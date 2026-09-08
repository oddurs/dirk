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
    /// Where *this* viewer is looking.
    ///
    /// Focus stopped being a property of the session when there could be more
    /// than one client: the whole point of two of them is a laptop and a
    /// monitor showing different parts of the same work.
    pub focus: crate::mux::Focus,
    /// And what this viewer's nav is doing, for the same reason. A selection is
    /// where somebody is pointing, and two people point at different things.
    pub nav: crate::ui::nav::Nav,
    /// The rows this viewer's nav last drew, so a key from this client acts on
    /// what this client can see.
    pub rows: Vec<crate::ui::nav::Row>,
    /// How big this client's screen is.
    area: ratatui::layout::Rect,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("View")
    }
}

impl View {
    /// How big this client's screen is.
    ///
    /// Kept here rather than asked of the terminal. `Terminal::size` asks the
    /// *backend*, and this backend is a socket -- so crossterm asks the process,
    /// which in a server with no controlling terminal answers 80x24. That is
    /// the same trap `Viewport::Fixed` exists to avoid, one level up.
    pub fn size(&self) -> ratatui::layout::Rect {
        self.area
    }

    /// Note a new size, and redraw against it.
    pub fn resized(&mut self, cols: u16, rows: u16) {
        self.area = ratatui::layout::Rect::new(0, 0, cols.max(1), rows.max(1));
        let _ = self.term.resize(self.area);
        // The old contents are wrong at the new size, and ratatui would
        // otherwise only send what changed against them.
        let _ = self.repaint();
    }

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
            // Corrected to somewhere real the moment the session sees it: a new
            // client should arrive looking at work rather than at whichever
            // board happens to be first.
            focus: crate::mux::Focus::Layout(0),
            nav: crate::ui::nav::Nav::default(),
            rows: Vec::new(),
            area,
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

    /// Put bytes in with the frame that is about to go.
    ///
    /// Into the same sink the renderer writes to, so they arrive in one message
    /// and in order: images placed in a separate write could reach the terminal
    /// before the cells they are drawn over.
    pub fn write(&mut self, bytes: &[u8]) {
        use io::Write;
        let _ = self.sink.clone().write_all(bytes);
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

/// One terminal attached to one pane, with no interface around it.
///
/// Not a [`View`]: a view is a whole session composed into a frame, and this is
/// one pane's own screen posted straight through. The difference is the point —
/// somebody attaching this way has a terminal too small for a sidebar, or is
/// already inside another multiplexer, or wants the pane and nothing else.
pub struct Watcher {
    pub id: u64,
    pub out: UnixStream,
    /// The pane, as the caller named it. Resolved by the loop that owns the
    /// session, because only it knows what exists.
    pub target: String,
    pub cols: u16,
    pub rows: u16,
    pub takeover: bool,
    /// Set once the loop has accepted it and resolved the pane.
    pub pane: crate::mux::PaneId,
    /// What was last posted, so an unchanged screen is not sent again.
    ///
    /// A pane redraws on a timer more often than it changes, and a direct
    /// attach that reposted an identical screen every tick would make a still
    /// terminal look like a flickering one over ssh.
    pub last: Vec<u8>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Watcher")
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
        Kind::Watch => watch(out, reader, tx, &body),
        Kind::Command => {
            let mut out = out;
            if answer(&mut out, &reader, &tx, &body).is_err() {
                return;
            }
            // The connection stays open for more, so a caller making several
            // requests pays for one connection rather than one each.
            while let Ok(Some((Kind::Command, body))) = wire::recv(&mut reader) {
                if answer(&mut out, &reader, &tx, &body).is_err() {
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
        if tx.send(Ev::Term(Some(id), event)).is_err() {
            return;
        }
    }
    // Named, so a client that was taken over cannot clear the view of the one
    // that replaced it on its way out.
    let _ = tx.send(Ev::Detach(id));
}

/// Serve one pane to one terminal.
fn watch(out: UnixStream, mut reader: UnixStream, tx: Sender<Ev>, body: &[u8]) {
    let Ok(want) = serde_json::from_slice::<wire::Watch>(body) else {
        return;
    };
    let id = next_client_id();
    // Same reason a view sets one: a terminal that stops draining must not be
    // able to hold the loop that owns every other pane.
    let _ = out.set_write_timeout(Some(std::time::Duration::from_secs(5)));
    let watcher = Watcher {
        id,
        out,
        target: want.pane,
        cols: want.cols,
        rows: want.rows,
        takeover: want.takeover,
        pane: 0,
        last: Vec::new(),
    };

    // Asked and answered before anything is streamed, because "no such pane"
    // and "somebody else has it" are the two things the caller has to be told
    // in a form it can read, and a stream is not that form.
    //
    // Answered by the loop rather than here: from the moment it holds a watcher
    // it is the thread writing frames down this socket, and a reply written
    // from this one could land in the middle of one.
    let (back, wait) = std::sync::mpsc::sync_channel(1);
    if tx
        .send(Ev::Watch {
            watcher: Box::new(watcher),
            back,
        })
        .is_err()
    {
        return;
    }
    if !wait.recv().unwrap_or(false) {
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
        if tx.send(Ev::Watched(id, event)).is_err() {
            return;
        }
    }
    let _ = tx.send(Ev::Unwatch(id));
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
fn answer(
    out: &mut UnixStream,
    reader: &UnixStream,
    tx: &Sender<Ev>,
    body: &[u8],
) -> io::Result<()> {
    let reply = match serde_json::from_slice::<wire::Request>(body) {
        Ok(req) => {
            let (back, wait) = std::sync::mpsc::sync_channel(1);
            let (answer, caller) = wire::Answer::pair(back);
            match tx.send(Ev::Command(req, answer)) {
                Ok(()) => await_reply(&wait, reader, caller)?,
                Err(_) => wire::Reply::err("the session went away"),
            }
        }
        Err(e) => wire::Reply::err(format!("not a request: {e}")),
    };
    wire::send_json(out, Kind::Reply, &reply)
}

/// How often a thread waiting on the session looks up to see if its caller is
/// still there. Cheap: one syscall, and only while an answer is outstanding.
const LOOK_UP: std::time::Duration = std::time::Duration::from_secs(1);

/// Wait for the session's answer, or for the caller to stop wanting it.
///
/// `pane wait-output` with no `--timeout` waits for as long as it takes, which
/// is what it is for. Before this, a caller that gave up — Ctrl-C, or the
/// script that ran it dying — left the thread blocked here for the life of the
/// session, holding its socket, and left the session holding a question it
/// re-examined on every turn of its loop for an answer nobody would read.
///
/// So the wait is in steps, and between them the socket is asked whether the
/// far end is still open. Returning drops `caller`, which is how the session
/// finds out.
fn await_reply(
    wait: &std::sync::mpsc::Receiver<wire::Reply>,
    reader: &UnixStream,
    caller: std::sync::Arc<()>,
) -> io::Result<wire::Reply> {
    loop {
        match wait.recv_timeout(LOOK_UP) {
            Ok(reply) => return Ok(reply),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Ok(wire::Reply::err("the session went away"));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if hung_up(reader) {
                    drop(caller);
                    return Err(io::Error::from(io::ErrorKind::BrokenPipe));
                }
            }
        }
    }
}

/// Has the far end of this connection closed?
///
/// Peeked rather than read: anything already there is the caller's next
/// request, and this connection stays open for more. Zero bytes is the end of
/// the stream and nothing else is — `WouldBlock` is an idle caller, which is
/// the ordinary state of one that is waiting.
///
/// `libc::recv` rather than `UnixStream::peek`, which is still unstable, and
/// `MSG_DONTWAIT` rather than a read timeout: the timeout is a property of the
/// socket, so setting one here would mean putting it back before the blocking
/// read that follows, and forgetting to would end that read early.
fn hung_up(sock: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut byte = [0u8; 1];
    // SAFETY: the descriptor belongs to `sock` and outlives the call; the
    // buffer is ours and the length passed is its own.
    let seen = unsafe {
        libc::recv(
            sock.as_raw_fd(),
            byte.as_mut_ptr().cast(),
            byte.len(),
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        )
    };
    match seen {
        0 => true,
        n if n > 0 => false,
        _ => !matches!(
            io::Error::last_os_error().kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ),
    }
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
