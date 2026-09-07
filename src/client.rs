// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along with
// this program.  If not, see <https://www.gnu.org/licenses/>.

//! The half that owns a terminal.
//!
//! It parses input, because that is where the terminal is, and it paints bytes
//! it does not read. Everything it knows about dirk is in `wire.rs`; everything
//! about what the screen means is on the other end of the socket.
//!
//! That asymmetry is the point. A client is small enough to be obviously
//! correct, and there is only one renderer to have bugs in.

use crate::wire::{self, Hello, Input, Kind};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

/// Attach to a session and stay until it or the user is done.
pub fn attach(path: &Path) -> io::Result<()> {
    let mut sock = UnixStream::connect(path)?;
    let mut reader = sock.try_clone()?;

    let (cols, rows) = crossterm::terminal::size()?;
    wire::send_json(&mut sock, Kind::Hello, &Hello { cols, rows })?;

    setup()?;
    let result = pump(&mut sock, &mut reader);
    restore();
    result
}

fn setup() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

    // A panic in raw mode leaves the terminal unusable and the backtrace
    // unreadable. Restore first, then let the default hook print.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        default(info);
    }));
    Ok(())
}

pub fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
}

fn pump(sock: &mut UnixStream, reader: &mut UnixStream) -> io::Result<()> {
    // Input on its own thread: reading the terminal blocks, and so does reading
    // the socket, and neither may wait for the other.
    let mut input = sock.try_clone()?;
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            let Some(msg) = Input::from_event(&event) else {
                continue;
            };
            if wire::send_json(&mut input, Kind::Input, &msg).is_err() {
                return;
            }
        }
    });

    let mut out = io::stdout();
    while let Some((kind, body)) = wire::recv(reader)? {
        match kind {
            // Bytes it does not read. The server has already worked out what
            // needs to change; this is the part that does not need to know.
            Kind::Frame => {
                out.write_all(&body)?;
                out.flush()?;
            }
            Kind::Bye => {
                let why: String = serde_json::from_slice(&body).unwrap_or_default();
                restore();
                if !why.is_empty() {
                    eprintln!("dirk: {why}");
                }
                return Ok(());
            }
            _ => {}
        }
    }
    Ok(())
}
