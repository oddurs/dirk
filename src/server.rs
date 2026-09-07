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

//! The half that owns the panes.
//!
//! It has no terminal of its own. It renders into a byte buffer — ratatui's
//! backend does not care that the far end is a socket rather than a tty — and
//! posts the escape sequences to whichever client is attached. There is
//! therefore exactly one renderer, and a bug in it cannot be present locally and
//! absent remotely.
//!
//! One client at a time. A second attach takes the session over and the first is
//! told why, which is the honest version of "does not fight": a view per client
//! needs a focus per client, and that is a change to what a session is rather
//! than to how it is reached.

use crate::mux::Ev;
use crate::wire::{self, Hello, Input, Kind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// Where a session's socket lives.
///
/// Under the user's own directory, created with permissions that keep it there:
/// anyone who can open this socket can type into every shell in the session.
pub fn socket_path(name: &str) -> PathBuf {
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir()
        .join(format!("dirk-{uid}"))
        .join(format!("{name}.sock"))
}

/// Is a server already listening here?
///
/// A socket file outliving its server is the normal case after a crash, so the
/// question is answered by connecting rather than by the file existing.
pub fn is_running(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

/// Bind the socket, replacing one nobody is listening on.
pub fn bind(path: &Path) -> io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        // The directory is the boundary. Anyone who can reach the socket can
        // type into every shell in the session.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    if path.exists() && !is_running(path) {
        // Left by a server that is gone. Its own socket is not a reason to
        // refuse to start.
        let _ = std::fs::remove_file(path);
    }
    UnixListener::bind(path)
}

/// Where a draw's escape sequences collect until they are posted.
///
/// Shared with the backend rather than owned by it: ratatui hands its writer to
/// the backend and does not hand it back, so the only way to read what a draw
/// produced is to have kept a hold of the other end.
#[derive(Clone, Default)]
pub struct Sink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("sink poisoned"))?
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Sink {
    fn take(&self) -> Vec<u8> {
        self.0
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default()
    }
}

/// A client, and the screen it is being shown.
pub struct View {
    pub out: UnixStream,
    pub term: Terminal<CrosstermBackend<Sink>>,
    sink: Sink,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("View")
    }
}

impl View {
    fn new(out: UnixStream, cols: u16, rows: u16) -> io::Result<Self> {
        let sink = Sink::default();
        let mut term = Terminal::new(CrosstermBackend::new(sink.clone()))?;
        term.resize(ratatui::layout::Rect::new(0, 0, cols.max(1), rows.max(1)))?;
        Ok(Self { out, term, sink })
    }

    /// Say the whole screen again.
    ///
    /// On attach the client's terminal holds whatever was there before, and
    /// ratatui would only send what changed since the *last* draw — which is
    /// nothing it knows about.
    pub fn repaint(&mut self) -> io::Result<()> {
        self.term.clear()
    }

    /// Post whatever the last draw produced.
    ///
    /// Taken rather than copied: the buffer is the message, and leaving it
    /// behind would send every frame again on the next one.
    pub fn flush(&mut self) -> io::Result<()> {
        let bytes = self.sink.take();
        if bytes.is_empty() {
            return Ok(());
        }
        wire::send(&mut self.out, Kind::Frame, &bytes)
    }
}

/// Take connections and turn them into events.
///
/// One thread accepts; one more per client reads its input. Both post into the
/// same channel the panes use, so the event loop has a single thing to wait on.
pub fn listen(listener: UnixListener, tx: Sender<Ev>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let Ok(reader) = stream.try_clone() else {
                continue;
            };
            let tx = tx.clone();
            std::thread::spawn(move || client(stream, reader, tx));
        }
    });
}

fn client(out: UnixStream, mut reader: UnixStream, tx: Sender<Ev>) {
    // The first message says how big the terminal is. Anything else is not a
    // dirk client.
    let Ok(Some((Kind::Hello, body))) = wire::recv(&mut reader) else {
        return;
    };
    let Ok(hello) = serde_json::from_slice::<Hello>(&body) else {
        return;
    };
    let Ok(view) = View::new(out, hello.cols, hello.rows) else {
        return;
    };
    if tx.send(Ev::Attach(Box::new(view))).is_err() {
        return;
    }

    while let Ok(Some((kind, body))) = wire::recv(&mut reader) {
        if kind != Kind::Input {
            continue;
        }
        let Ok(input) = serde_json::from_slice::<Input>(&body) else {
            continue;
        };
        let Some(event) = input.into_event() else {
            continue;
        };
        if tx.send(Ev::Term(event)).is_err() {
            return;
        }
    }
    let _ = tx.send(Ev::Detach);
}

/// Can anyone still reach this server?
///
/// A socket file can go without the server going with it — another server
/// tidying up, or a hand removing it. What is left holds every shell in the
/// session and nothing can ever connect to it again, so it should not keep
/// running. Checked rather than assumed, because the alternative is a process
/// that only `kill` can end and only `ps` can find.
pub fn reachable(path: &Path) -> bool {
    path.exists()
}

/// Start a server for this session in the background, and wait until its socket
/// answers.
///
/// dirk re-executes itself rather than forking: a process with a thread per pane
/// is not one to fork by hand, and `pre_exec` runs `setsid` in the child before
/// anything of ours does, which is what makes the session survive the terminal
/// that started it.
pub fn spawn(name: &str, path: &Path) -> io::Result<()> {
    use std::os::unix::process::CommandExt;

    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("server")
        .arg("--session")
        .arg(name)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            // Its own session, so closing the terminal that started it does not
            // take it down with a hangup.
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()?;

    // Waited for rather than assumed: attaching to a socket that is not there
    // yet is the first thing that would happen otherwise.
    for _ in 0..100 {
        if is_running(path) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(io::Error::other("the server did not start"))
}
