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
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Reader = Box<dyn Read + Send>;
type Writer = Box<dyn Write + Send>;

/// Where input goes at the moment.
///
/// Swapped when a link is remade, and empty in between. A key pressed at a dead
/// link is dropped rather than queued: it belonged to the screen that was there
/// when it was pressed, and delivering it to whatever comes back is how you
/// answer a prompt you never saw.
#[derive(Default)]
struct Link(Option<Writer>);

impl Link {
    fn send(&mut self, msg: &Input) {
        let Some(out) = self.0.as_mut() else { return };
        if wire::send_json(out, Kind::Input, msg).is_err() {
            self.0 = None;
        }
    }
}

/// How a link is made, and whether losing one is worth waiting out.
struct Dial<'a> {
    open: &'a mut dyn FnMut() -> io::Result<(Reader, Writer)>,
    /// A local socket that stops answering means the server is gone. A remote
    /// one may only mean a network, which comes back.
    patient: bool,
}

/// Attach to a session on this machine and stay until it or the user is done.
pub fn attach(path: &Path) -> io::Result<()> {
    let path = path.to_path_buf();
    run(&mut Dial {
        patient: false,
        open: &mut || {
            let sock = UnixStream::connect(&path)?;
            let reader = sock.try_clone()?;
            Ok((Box::new(reader) as Reader, Box::new(sock) as Writer))
        },
    })
    .map(|_| ())
}

/// Attach to a session on another machine, over ssh.
///
/// ssh is the whole authentication story: dirk has no transport of its own and
/// no business inventing one. What crosses is exactly what crosses a socket,
/// which is why this is a transport and not a second client.
pub fn remote(target: &str, session: &str) -> io::Result<()> {
    let (target, session) = (target.to_string(), session.to_string());
    let child: Arc<Mutex<Option<std::process::Child>>> = Arc::default();
    // Kept rather than shown: the screen belongs to the session, and what ssh
    // has to say is worth saying only if there was never a session to show.
    let said: Arc<Mutex<String>> = Arc::default();
    let (kept, heard) = (Arc::clone(&child), Arc::clone(&said));

    let painted = run(&mut Dial {
        patient: true,
        open: &mut || dial_ssh(&target, &session, &kept, &heard),
    });

    if let Ok(mut c) = child.lock()
        && let Some(mut ch) = c.take()
    {
        let _ = ch.kill();
        let _ = ch.wait();
    }
    // ssh's own account of why. dirk has nothing to add to "Could not resolve
    // hostname", and inventing a message of its own would only hide that one.
    if !matches!(painted, Ok(true))
        && let Ok(said) = said.lock()
        && !said.trim().is_empty()
    {
        eprint!("{}", said.trim_start());
    }
    painted.map(|_| ())
}

fn dial_ssh(
    target: &str,
    session: &str,
    kept: &Arc<Mutex<Option<std::process::Child>>>,
    heard: &Arc<Mutex<String>>,
) -> io::Result<(Reader, Writer)> {
    // Whatever was there before. ssh does not notice that nobody is reading it,
    // and a long session of dropped links would be a long session of processes.
    if let Ok(mut c) = kept.lock()
        && let Some(mut old) = c.take()
    {
        let _ = old.kill();
        let _ = old.wait();
    }

    // Overridable because "ssh" is not always the program that gets you there:
    // a wrapper that adds a jump host or a `-F` of its own is somebody's normal
    // way in, and it takes the same arguments.
    let ssh = std::env::var("DIRK_SSH").unwrap_or_else(|_| "ssh".into());
    let mut cmd = std::process::Command::new(ssh);
    cmd
        // No pty. Both ends speak a length-prefixed binary protocol, and a line
        // discipline in the middle of one rewrites it.
        .arg("-T")
        .arg("--")
        .arg(target)
        // Not on `PATH` everywhere, and a remote install is not ours to move.
        .arg(std::env::var("DIRK_REMOTE").unwrap_or_else(|_| "dirk".into()))
        .arg("--session")
        .arg(session)
        .arg("relay")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    let (Some(out), Some(inn)) = (child.stdout.take(), child.stdin.take()) else {
        return Err(io::Error::other("ssh gave us no pipes"));
    };
    if let Some(mut err) = child.stderr.take() {
        let heard = Arc::clone(heard);
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err.read_to_string(&mut buf);
            if let Ok(mut said) = heard.lock() {
                *said = buf;
            }
        });
    }
    if let Ok(mut c) = kept.lock() {
        *c = Some(child);
    }
    Ok((Box::new(out) as Reader, Box::new(inn) as Writer))
}

/// The far side of a remote attach: a session socket, on stdin and stdout.
///
/// Deliberately not a second client. It copies bytes, so a client and a server
/// of different versions disagree about the protocol and not about this.
pub fn relay(path: &Path) -> io::Result<()> {
    let sock = UnixStream::connect(path)?;
    let mut up = sock.try_clone()?;
    let down = sock;
    let back = std::thread::spawn(move || copy(down, io::stdout()));
    let _ = copy(io::stdin(), &mut up);
    // Said rather than left to a timeout: the session should learn that its
    // client has gone at the moment ssh does.
    let _ = up.shutdown(Shutdown::Both);
    let _ = back.join();
    Ok(())
}

/// `io::copy`, flushing every chunk.
///
/// stdout is a `LineWriter` and the wire is binary: a frame would sit in the
/// buffer until some byte of a later one happened to be a newline.
fn copy(mut from: impl Read, mut to: impl Write) -> io::Result<()> {
    let mut buf = [0u8; 32 * 1024];
    loop {
        match from.read(&mut buf)? {
            0 => return Ok(()),
            n => {
                to.write_all(&buf[..n])?;
                to.flush()?;
            }
        }
    }
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

/// Own the terminal, and hand it to one link after another.
///
/// Answers whether a frame ever arrived. A link that never painted anything is
/// not a dropped connection, it is a configuration that does not work, and the
/// difference decides both whether to wait for it and whether the caller owes
/// the user an explanation.
fn run(dial: &mut Dial) -> io::Result<bool> {
    let link: Arc<Mutex<Link>> = Arc::default();
    // One for the life of the process, not one per link. `crossterm::event::read`
    // is a queue with a single consumer, and two threads reading it would take
    // some of the keys each.
    let input = Arc::clone(&link);
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            let Some(msg) = Input::from_event(&event) else {
                continue;
            };
            let Ok(mut link) = input.lock() else { return };
            link.send(&msg);
        }
    });

    // Before the terminal is touched, so a connection that fails reports itself
    // the way any other program would rather than on a screen that is about to
    // be thrown away.
    let mut reader = connect(dial, &link)?;
    setup()?;

    let mut ever = false;
    loop {
        let end = pump(&mut reader);
        if let Ok(mut link) = link.lock() {
            link.0 = None;
        }
        let painted = match &end {
            Ok(End::Bye) => break,
            Ok(End::Dropped { painted }) => *painted,
            Err(_) => false,
        };
        ever |= painted;
        // Nothing to go back to, or nothing that was ever there.
        if !dial.patient || !(ever || painted) {
            if let Err(e) = end {
                restore();
                return Err(e);
            }
            break;
        }
        match again(dial, &link) {
            Some(next) => reader = next,
            None => break,
        }
    }
    restore();
    Ok(ever)
}

/// Make a link and tell the session how big the terminal is.
fn connect(dial: &mut Dial, link: &Arc<Mutex<Link>>) -> io::Result<Reader> {
    let (reader, mut writer) = (dial.open)()?;
    // Asked now rather than remembered: a terminal resized while the link was
    // down would otherwise be drawn at the size it used to be.
    let (cols, rows) = crossterm::terminal::size()?;
    wire::send_json(&mut writer, Kind::Hello, &Hello { cols, rows })?;
    if let Ok(mut link) = link.lock() {
        link.0 = Some(writer);
    }
    Ok(reader)
}

/// Wait out a dropped link, saying so while it waits.
///
/// Backing off rather than hammering: the usual reason is a laptop lid, and the
/// answer to that is to still be here when it opens.
fn again(dial: &mut Dial, link: &Arc<Mutex<Link>>) -> Option<Reader> {
    // Two minutes of trying, spaced out. A closed lid is measured in minutes
    // and a client that gave up after thirty seconds is one you would rather
    // had waited: the session is still there either way, but coming back to it
    // yourself means finding out that you have to.
    for (n, wait) in [1, 1, 2, 4, 8, 15, 30, 30, 30].into_iter().enumerate() {
        notice(&format!("reconnecting ({})", n + 1));
        std::thread::sleep(Duration::from_secs(wait));
        if let Ok(reader) = connect(dial, link) {
            return Some(reader);
        }
    }
    notice("could not reconnect");
    None
}

/// A word at the top left, over whatever the last frame left there.
///
/// The next frame is a full repaint, so the row is only borrowed -- and a
/// client that says nothing while it waits is indistinguishable from one that
/// has hung.
fn notice(text: &str) {
    let mut out = io::stdout();
    let _ = write!(out, "\x1b7\x1b[1;1H\x1b[7m dirk: {text} \x1b[0m\x1b8");
    let _ = out.flush();
}

/// Why a link stopped.
enum End {
    /// The session said so, and meant it.
    Bye,
    /// It stopped answering.
    Dropped { painted: bool },
}

/// Read frames until the session says goodbye or the link stops.
fn pump(reader: &mut dyn Read) -> io::Result<End> {
    let mut out = io::stdout();
    let mut painted = false;
    while let Some((kind, body)) = wire::recv(reader)? {
        match kind {
            // Bytes it does not read. The server has already worked out what
            // needs to change; this is the part that does not need to know.
            Kind::Frame => {
                out.write_all(&body)?;
                out.flush()?;
                painted = true;
            }
            Kind::Bye => {
                let why: String = serde_json::from_slice(&body).unwrap_or_default();
                restore();
                if !why.is_empty() {
                    eprintln!("dirk: {why}");
                }
                return Ok(End::Bye);
            }
            _ => {}
        }
    }
    Ok(End::Dropped { painted })
}
