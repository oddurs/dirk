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
    /// Which client this is.
    ///
    /// A detach carries one too, so the reader thread of a client that has been
    /// taken over cannot clear the view of the one that replaced it.
    pub id: u64,
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
    fn new(id: u64, out: UnixStream, cols: u16, rows: u16) -> io::Result<Self> {
        let sink = Sink::default();
        // Fixed, not fullscreen. A fullscreen viewport re-asks the backend for
        // the size before every draw, and the backend asks the *process* --
        // which in a server with no controlling terminal fails, falls back to
        // `tput`, and answers 80x24. Every draw would then resize the client's
        // screen back to that, so anyone on a larger terminal would get an
        // 80x24 dirk and resizing the window would do nothing. It also shelled
        // out twice per frame to find that out.
        let area = ratatui::layout::Rect::new(0, 0, cols.max(1), rows.max(1));
        let options = ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        };
        let term = Terminal::with_options(CrosstermBackend::new(sink.clone()), options)?;

        // A client that stops draining must not take the session with it. The
        // event loop writes frames itself, and a socket whose buffer is full
        // would block it for ever -- no input, no tick, no takeover, no quit.
        let _ = out.set_write_timeout(Some(std::time::Duration::from_secs(5)));
        Ok(Self {
            id,
            out,
            term,
            sink,
        })
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
    // The first message says what kind of connection this is. A viewer says
    // hello with its size; a caller asks something.
    let Ok(Some((kind, body))) = wire::recv(&mut reader) else {
        return;
    };
    match kind {
        Kind::Hello => view(out, reader, tx, &body),
        Kind::Command => {
            let mut out = out;
            if answer(&mut out, &tx, &body).is_err() {
                return;
            }
            // The connection stays open for more, so a caller making several
            // requests pays for one connection rather than one each.
            while let Ok(Some((Kind::Command, body))) = wire::recv(&mut reader) {
                if answer(&mut out, &tx, &body).is_err() {
                    return;
                }
            }
        }
        _ => {}
    }
}

fn view(out: UnixStream, mut reader: UnixStream, tx: Sender<Ev>, body: &[u8]) {
    let Ok(hello) = serde_json::from_slice::<Hello>(body) else {
        return;
    };
    let id = next_client_id();
    let Ok(view) = View::new(id, out, hello.cols, hello.rows) else {
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
    // Named, so a client that was taken over cannot clear the view of the one
    // that replaced it on its way out.
    let _ = tx.send(Ev::Detach(id));
}

fn next_client_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Put one request to the loop that owns the state, and post what it says.
///
/// Answered there rather than here so a command sees the session between
/// frames, never halfway through one.
fn answer(out: &mut UnixStream, tx: &Sender<Ev>, body: &[u8]) -> io::Result<()> {
    let reply = match serde_json::from_slice::<wire::Request>(body) {
        Ok(req) => {
            let (back, wait) = std::sync::mpsc::sync_channel(1);
            match tx.send(Ev::Command(req, back)) {
                Ok(()) => wait
                    .recv()
                    .unwrap_or_else(|_| wire::Reply::err("the session went away")),
                Err(_) => wire::Reply::err("the session went away"),
            }
        }
        Err(e) => wire::Reply::err(format!("not a request: {e}")),
    };
    wire::send_json(out, Kind::Reply, &reply)
}

/// Ask a running session something, from outside it.
pub fn ask(path: &Path, req: &wire::Request) -> io::Result<wire::Reply> {
    let mut sock = UnixStream::connect(path)?;
    wire::send_json(&mut sock, Kind::Command, req)?;
    let mut reader = sock.try_clone()?;
    match wire::recv(&mut reader)? {
        Some((Kind::Reply, body)) => serde_json::from_slice(&body).map_err(io::Error::other),
        _ => Err(io::Error::other("the session did not answer")),
    }
}

/// Can anyone still reach this server?
///
/// A socket file can go without the server going with it — another server
/// tidying up, or a hand removing it. What is left holds every shell in the
/// session and nothing can ever connect to it again, so it should not keep
/// running. Checked rather than assumed, because the alternative is a process
/// that only `kill` can end and only `ps` can find.
pub fn reachable(path: &Path, ours: u64) -> bool {
    use std::os::unix::fs::MetadataExt;
    // The inode, not the path. A socket can be removed and another server can
    // bind a fresh one at the same name before the next tick -- and a server
    // that only asked whether *something* is there would see the newcomer's
    // socket and keep running, holding every pane behind an address that now
    // belongs to somebody else.
    std::fs::metadata(path)
        .map(|m| m.ino())
        .is_ok_and(|ino| ino == ours)
}

/// The inode of the socket just bound, as proof of which one is ours.
pub fn inode(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|m| m.ino())
}

/// Every session with a socket, and whether anyone is listening on it.
///
/// Answered by connecting rather than by the directory listing: a socket file
/// outliving its server is the ordinary state of affairs after a crash, and a
/// list that reported those as sessions would be a list of things you cannot
/// attach to.
pub fn sessions() -> Vec<(String, bool)> {
    let dir = socket_path("x")
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut out: Vec<(String, bool)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_str()?.to_string();
            (path.extension()? == "sock").then(|| (name, is_running(&path)))
        })
        .collect();
    out.sort();
    out
}

/// A session name has to be one path segment.
///
/// It is interpolated into a path that is later removed, so `../../.ssh/config`
/// would be resolved rather than refused. Same-user throughout, so this is a
/// footgun rather than a boundary — but a footgun with a trigger guard.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && name != "."
        && name != ".."
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
        // A server has no terminal to complain to, so by default its stderr
        // goes nowhere. `DIRK_DEBUG=<file>` gives a panic somewhere to land:
        // without it a server that dies mid-frame leaves only a session that
        // stopped answering.
        .stderr(
            match std::env::var("DIRK_DEBUG").ok().and_then(|p| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .ok()
            }) {
                Some(f) => std::process::Stdio::from(f),
                None => std::process::Stdio::null(),
            },
        );
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
