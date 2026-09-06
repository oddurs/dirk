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

//! `cargo run --example shot` — what dirk actually paints, as text.
//!
//! Spawns the real binary on a pty, feeds its output through the same terminal
//! parser dirk uses for its own panes, and prints the resulting screen. Useful
//! for looking at the chrome without being inside it.

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let (rows, cols) = (26u16, 92u16);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/dirk"));
    cmd.cwd(env!("CARGO_MANIFEST_DIR"));
    cmd.env("TERM", "xterm-256color");
    cmd.env("SHELL", "/bin/sh");
    // `DIRK_SHOT_CONFIG=<dir> cargo run --example shot` points dirk at a
    // configuration directory of your own, which is how a layout gets looked at
    // without installing it.
    if let Ok(dir) = std::env::var("DIRK_SHOT_CONFIG") {
        cmd.env("XDG_CONFIG_HOME", dir);
    }
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    let screen = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
    let sink = Arc::clone(&screen);
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                return;
            }
            sink.lock().unwrap().process(&buf[..n]);
        }
    });

    // Let it come up, then give it something to look at: two workspaces with
    // titles for naming to work from, and a split to show the layout tree.
    //
    // The split comes last on purpose. `name::decide` skips a workspace holding
    // more than one pane -- two panes have no single intent -- so splitting
    // before the title has settled would leave that workspace showing its
    // project name forever, and the shot would quietly stop demonstrating the
    // one feature it was written for.
    let title = |t: &str| format!("printf '\\033]2;{t}\\007'\r");
    let prefix = |k: &[u8]| [&[0u8][..], k].concat();

    std::thread::sleep(Duration::from_millis(700));
    writer
        .write_all(title("Building the mux core").as_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));

    writer.write_all(&prefix(b"n")).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    writer
        .write_all(title("Reading the vt100 grid").as_bytes())
        .unwrap();

    // Longer than the naming debounce, so both names are committed.
    std::thread::sleep(Duration::from_millis(2000));

    writer.write_all(&prefix(b"|")).unwrap();
    std::thread::sleep(Duration::from_millis(700));

    if std::env::var("DIRK_SHOT_LAYOUT").is_ok() {
        writer.write_all(&prefix(b"1")).unwrap();
        std::thread::sleep(Duration::from_millis(2500));
    }

    for row in 0..rows {
        let s = screen.lock().unwrap();
        let line: String = (0..cols)
            .map(|c| {
                s.screen()
                    .cell(row, c)
                    .map(|x| {
                        let t = x.contents();
                        if t.is_empty() {
                            " ".to_string()
                        } else {
                            t.to_string()
                        }
                    })
                    .unwrap_or_else(|| " ".into())
            })
            .collect();
        println!("│{}│", line.trim_end());
    }

    writer.write_all(&[0]).unwrap();
    writer.write_all(b"q").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = child.kill();
}
