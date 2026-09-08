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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Reader = Box<dyn Read + Send>;
type Writer = Box<dyn Write + Send>;

/// What the terminal and the main loop tell the one thread that writes.
enum ToLink {
    /// A terminal event, for the session.
    Event(Input),
    /// A link was made. Whatever was there before is dropped.
    Open(Writer),
    /// There is nowhere to send until further notice.
    Closed,
}

/// The only thread that writes to a link.
///
/// A channel rather than a lock around the writer, because a write to a stalled
/// link blocks for as long as the link is stalled. A lock held across that is a
/// lock the main loop waits on, so the client could neither drop the dead link
/// nor make a new one -- which is the case this whole thing exists for.
fn spawn_writer(rx: Receiver<ToLink>, leave: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut out: Option<Writer> = None;
        while let Ok(msg) = rx.recv() {
            match msg {
                ToLink::Open(w) => out = Some(w),
                ToLink::Closed => out = None,
                ToLink::Event(ev) => match out.as_mut() {
                    Some(w) => {
                        if wire::send_json(w, Kind::Input, &ev).is_err() {
                            out = None;
                        }
                    }
                    // Nowhere to send. A key pressed at a dead link belonged to
                    // the screen that was there when it was pressed, so it is
                    // dropped rather than delivered to whatever comes back --
                    // except the one that means "stop waiting", which is
                    // otherwise unsayable while the link is down.
                    None => {
                        if leaving(&ev) {
                            leave.store(true, Ordering::Relaxed);
                        }
                    }
                },
            }
        }
    });
}

/// Ctrl-C, which is the only key with a meaning of its own on this side.
fn leaving(ev: &Input) -> bool {
    const CONTROL: u8 = 0b0000_0010;
    matches!(ev, Input::Key { code, mods, release }
        if code == "c" && mods & CONTROL != 0 && !*release)
}

/// Reading the terminal, once there is a session to read it for.
fn spawn_input(tx: Sender<ToLink>) {
    // One for the life of the process, not one per link.
    // `crossterm::event::read` is a queue with a single consumer, and two
    // threads reading it would take some of the keys each.
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            let Some(msg) = Input::from_event(&event) else {
                continue;
            };
            if tx.send(ToLink::Event(msg)).is_err() {
                return;
            }
        }
    });
}

/// How a link is made, and whether losing one is worth waiting out.
struct Dial<'a> {
    /// Makes one. The flag is whether this is the first attempt, which is the
    /// only one that still has a terminal to talk to.
    open: &'a mut dyn FnMut(bool) -> io::Result<(Reader, Writer)>,
    /// A local socket that stops answering means the server is gone. A remote
    /// one may only mean a network, which comes back.
    patient: bool,
}

/// Attach to a session on this machine and stay until it or the user is done.
pub fn attach(path: &Path) -> io::Result<()> {
    let path = path.to_path_buf();
    run(&mut Dial {
        patient: false,
        open: &mut |_| {
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
    let kept = Arc::clone(&child);

    let reached = run(&mut Dial {
        patient: true,
        open: &mut |first| dial_ssh(&target, &session, &kept, first),
    });

    // Whatever is still out there. ssh does not notice that nobody is reading
    // it, and a client that exits leaving one behind leaves the far session
    // attached to a client that has gone.
    if let Ok(mut c) = child.lock()
        && let Some(mut ch) = c.take()
    {
        let _ = ch.kill();
        let _ = ch.wait();
    }

    match reached {
        Ok(true) => Ok(()),
        // ssh has already said why, on the terminal it was given for exactly
        // that. dirk has nothing to add to "Could not resolve hostname" -- but
        // it does owe the shell a status that says it did not work.
        Ok(false) => Err(io::Error::other(format!("no session on {target}"))),
        Err(e) => Err(e),
    }
}

fn dial_ssh(
    target: &str,
    session: &str,
    kept: &Arc<Mutex<Option<std::process::Child>>>,
    first: bool,
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
        // The first attempt still has the terminal, and ssh may need it: a host
        // key to confirm, a passphrase, a second factor. Capturing that would
        // leave the user looking at a blank screen wondering what is wanted.
        // Later attempts happen on a screen that belongs to the session, so
        // there is nowhere for ssh to say anything and it is told so.
        .stderr(match first {
            true => std::process::Stdio::inherit(),
            false => std::process::Stdio::null(),
        });

    let mut child = cmd.spawn()?;
    let (Some(out), Some(inn)) = (child.stdout.take(), child.stdin.take()) else {
        return Err(io::Error::other("ssh gave us no pipes"));
    };
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
    let mut down = sock;

    std::thread::spawn(move || {
        let _ = copy(io::stdin(), &mut up);
        // Only the half we are done with. The session may still have something
        // to say -- a `Bye` and its reason is the usual thing -- and closing
        // both halves here throws that away on the way past.
        let _ = up.shutdown(Shutdown::Write);
    });

    // On this thread, because this is the direction that decides when the relay
    // is over. A session that has gone has to reach the client as a link that
    // has gone; ending only when stdin closes leaves a dead session looking
    // like a screen that stopped changing.
    copy(&mut down, io::stdout())
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
/// Answers whether a session was ever reached. A link that never delivered
/// anything is not a dropped connection, it is a configuration that does not
/// work, and the difference decides both whether to wait for it and what the
/// caller owes the shell on the way out.
fn run(dial: &mut Dial) -> io::Result<bool> {
    let (tx, rx) = mpsc::channel::<ToLink>();
    let leave = Arc::new(AtomicBool::new(false));
    spawn_writer(rx, Arc::clone(&leave));

    // Before the terminal is touched, and before anything reads it: ssh may
    // want the tty for a host key or a passphrase, and a second reader takes
    // half of what is typed at it.
    let mut reader = connect(dial, &tx, true)?;
    setup()?;

    // Started at the first frame rather than here, for the same reason.
    let listening = Arc::new(AtomicBool::new(false));
    let arrived: Box<dyn Fn()> = {
        let (listening, tx) = (Arc::clone(&listening), tx.clone());
        Box::new(move || {
            if !listening.swap(true, Ordering::Relaxed) {
                spawn_input(tx.clone());
            }
        })
    };

    let mut reached = false;
    let mut attempt = 0usize;
    let result = loop {
        let end = pump(&mut reader, arrived.as_ref());
        let _ = tx.send(ToLink::Closed);

        let painted = match end {
            Ok(End::Bye) => break Ok(true),
            Ok(End::Dropped { painted }) => painted,
            Err(e) if !dial.patient => break Err(e),
            Err(_) => false,
        };
        if painted {
            // A link that worked earns a fresh ladder. One that did not does
            // not, which is what stops a host that has gone for good from being
            // dialled once a second until somebody kills this from elsewhere.
            reached = true;
            attempt = 0;
        }
        if !dial.patient || !reached {
            break Ok(reached);
        }
        match again(dial, &tx, &mut attempt, &leave) {
            Some(next) => reader = next,
            None => break Ok(reached),
        }
    };
    restore();
    result
}

/// Make a link and tell the session how big the terminal is.
fn connect(dial: &mut Dial, tx: &Sender<ToLink>, first: bool) -> io::Result<Reader> {
    let (reader, mut writer) = (dial.open)(first)?;
    // Asked now rather than remembered: a terminal resized while the link was
    // down would otherwise be drawn at the size it used to be.
    let (cols, rows) = crossterm::terminal::size()?;
    wire::send_json(&mut writer, Kind::Hello, &Hello { cols, rows })?;
    let _ = tx.send(ToLink::Open(writer));
    Ok(reader)
}

/// How long to wait before each attempt. Two minutes in all: a closed lid is
/// measured in minutes, and a client that gave up after thirty seconds is one
/// you would rather had waited.
const LADDER: [u64; 9] = [1, 1, 2, 4, 8, 15, 30, 30, 30];

/// Wait out a dropped link, saying so while it waits.
///
/// The count belongs to the run rather than to this call. A link that comes
/// back and dies again without ever painting is not a fresh start, and starting
/// the ladder over each time is how a client ends up dialling a host that has
/// gone for good once a second, for ever.
fn again(
    dial: &mut Dial,
    tx: &Sender<ToLink>,
    attempt: &mut usize,
    leave: &AtomicBool,
) -> Option<Reader> {
    loop {
        let wait = LADDER.get(*attempt).copied()?;
        *attempt += 1;
        notice(&format!("reconnecting ({})", *attempt));
        if !nap(Duration::from_secs(wait), leave) {
            return None;
        }
        if let Ok(reader) = connect(dial, tx, false) {
            return Some(reader);
        }
    }
}

/// Sleep, unless the user has said they would rather not.
///
/// In slices, because raw mode makes Ctrl-C a key rather than a signal: a wait
/// with no way out of it is a client you have to kill from another terminal.
fn nap(total: Duration, leave: &AtomicBool) -> bool {
    let until = Instant::now() + total;
    loop {
        if leave.load(Ordering::Relaxed) {
            return false;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        std::thread::sleep(left.min(Duration::from_millis(100)));
    }
}

/// A word at the top left, over whatever the last frame left there.
///
/// The next frame is a full repaint, so the row is only borrowed -- and a
/// client that says nothing while it waits is indistinguishable from one that
/// has hung.
fn notice(text: &str) {
    let mut out = io::stdout();
    let _ = write!(
        out,
        "\x1b7\x1b[1;1H\x1b[7m dirk: {text} (ctrl-c to leave) \x1b[0m\x1b8"
    );
    let _ = out.flush();
}

/// This machine's configuration, read once.
///
/// A remote attach uses the near side's: the sound file lives on the machine
/// with the speakers, and which one it is was never the far side's business.
fn config() -> &'static crate::config::Config {
    static ONCE: std::sync::OnceLock<crate::config::Config> = std::sync::OnceLock::new();
    ONCE.get_or_init(crate::config::Config::load)
}

/// Why a link stopped.
enum End {
    /// The session said so, and meant it.
    Bye,
    /// It stopped answering.
    Dropped { painted: bool },
}

/// Read frames until the session says goodbye or the link stops.
fn pump(reader: &mut dyn Read, arrived: &dyn Fn()) -> io::Result<End> {
    let mut out = io::stdout();
    let mut painted = false;
    while let Some((kind, body)) = wire::recv(reader)? {
        match kind {
            // Bytes it does not read. The server has already worked out what
            // needs to change; this is the part that does not need to know.
            Kind::Frame => {
                out.write_all(&body)?;
                out.flush()?;
                if !painted {
                    painted = true;
                    arrived();
                }
            }
            // Performed here rather than in the session, because this is the
            // machine with the screen and the speakers. Attach over ssh and the
            // session is on the build box while the human is at a laptop, and
            // an interruption delivered where nobody is sitting is one nobody
            // gets.
            //
            // The session decided *whether*: it holds the state machine, the
            // seen rule and the floor. This end decides *how*, from its own
            // configuration -- which is also what keeps the wire from being a
            // way to ask the other end to run a command of the sender's
            // choosing.
            Kind::Alert => {
                let Ok(alert) = serde_json::from_slice::<wire::Alert>(&body) else {
                    continue;
                };
                let quiet = alert.state.ends_with("-quiet");
                let name = alert.state.trim_end_matches("-quiet");
                let Some(which) = crate::sound::Alert::named(name) else {
                    continue;
                };
                let what = match which {
                    crate::sound::Alert::Blocked => "is waiting for you",
                    crate::sound::Alert::Done => "has finished",
                };
                let cfg = config();
                let title = match alert.text.is_empty() {
                    true => format!("{} {what}", alert.label),
                    false => alert.text.clone(),
                };
                crate::notify::send(title, cfg.identity.app());
                if !quiet {
                    crate::sound::play(&cfg.sound, which);
                }
            }
            // Both ways, because neither reaches every terminal. The command
            // is the machine this client is running on; the escape sequence
            // asks the terminal, which is the one that crosses ssh.
            Kind::Clip => {
                let Ok(text) = serde_json::from_slice::<String>(&body) else {
                    continue;
                };
                out.write_all(&crate::clipboard::osc52(&text))?;
                out.flush()?;
                crate::clipboard::put(&config().clipboard, &text);
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

/// One pane, in this terminal, with no interface around it.
///
/// Attaching gives you the whole of dirk. Sometimes what you want is the one
/// pane the agent is in — over ssh from a phone, inside another multiplexer, or
/// in a terminal too small for a sidebar to be anything but in the way.
///
/// The prefix keeps its meaning, and means only two things here: `d` leaves,
/// and pressing it twice sends a literal one through to the program. Everything
/// else goes to the pane, which is the whole point.
pub fn watch(path: &Path, target: &str, takeover: bool) -> io::Result<()> {
    let mut sock = UnixStream::connect(path)?;
    let mut reader = sock.try_clone()?;
    let (cols, rows) = crossterm::terminal::size()?;
    wire::send_json(
        &mut sock,
        Kind::Watch,
        &wire::Watch {
            pane: target.to_string(),
            cols,
            rows,
            takeover,
        },
    )?;

    // Answered before the terminal is touched: "no such pane" and "somebody
    // else has it" are things to read on a normal screen, not through a
    // terminal that has just been put into raw mode and cleared.
    match wire::recv(&mut reader)? {
        Some((Kind::Reply, body)) => {
            let reply: wire::Reply = serde_json::from_slice(&body).map_err(io::Error::other)?;
            if !reply.ok {
                return Err(io::Error::other(reply.error.unwrap_or_default()));
            }
        }
        _ => return Err(io::Error::other("the session did not answer")),
    }

    setup()?;
    let result = pump_watch(&mut reader, sock);
    restore();
    result
}

/// Frames out, keys in, until somebody leaves.
fn pump_watch(reader: &mut UnixStream, writer: UnixStream) -> io::Result<()> {
    let leaving = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&leaving);
    let mut sending = writer;

    // Input on its own thread, because reading the terminal blocks and so does
    // reading the socket, and neither may wait for the other.
    let feeding = std::thread::spawn(move || {
        // dirk's prefix, and it carries its usual promise: pressed twice it
        // sends a literal one through, so a program that wants NUL still gets
        // it.
        let prefix = |k: &crossterm::event::KeyEvent| {
            k.code == crossterm::event::KeyCode::Char(' ')
                && k.modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
        };
        let mut armed = false;
        while let Ok(event) = crossterm::event::read() {
            if let crossterm::event::Event::Key(k) = &event
                && k.kind != crossterm::event::KeyEventKind::Release
            {
                if armed {
                    armed = false;
                    // `d`, as everywhere else in dirk. Leaving is the only
                    // thing this attachment has to offer beyond the pane.
                    if k.code == crossterm::event::KeyCode::Char('d') {
                        stop.store(true, Ordering::Relaxed);
                        // Its own socket, so the reader on the far side wakes
                        // up rather than waiting for a frame that is not coming.
                        let _ = sending.shutdown(std::net::Shutdown::Both);
                        return;
                    }
                    // Anything else after the prefix: if it was the prefix
                    // again, one goes through; otherwise both do, because
                    // swallowing a keystroke is worse than sending one.
                    if !prefix(k) {
                        let literal =
                            crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                                crossterm::event::KeyCode::Char(' '),
                                crossterm::event::KeyModifiers::CONTROL,
                            ));
                        if let Some(msg) = Input::from_event(&literal) {
                            let _ = wire::send_json(&mut sending, Kind::Input, &msg);
                        }
                    }
                } else if prefix(k) {
                    armed = true;
                    continue;
                }
            }
            let Some(msg) = Input::from_event(&event) else {
                continue;
            };
            if wire::send_json(&mut sending, Kind::Input, &msg).is_err() {
                return;
            }
        }
    });

    let out = io::stdout();
    loop {
        match wire::recv(reader) {
            Ok(Some((Kind::Frame, bytes))) => {
                let mut lock = out.lock();
                lock.write_all(&bytes)?;
                lock.flush()?;
            }
            Ok(Some((Kind::Bye, _))) | Ok(None) => break,
            Ok(Some(_)) => continue,
            Err(_) => break,
        }
    }
    // The input thread is blocked on the terminal and cannot be asked to stop;
    // it goes when the process does. What matters is that we do not wait for it.
    drop(feeding);
    match leaving.load(Ordering::Relaxed) {
        true => Ok(()),
        false => Err(io::Error::other("the pane went away")),
    }
}
