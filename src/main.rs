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

//! dirk — a multiplexer that knows what its sessions are for.
//!
//! The shape of the thing, in one screen:
//!
//! ```text
//! ┌──────────────┬──────────────────────────────────────┐
//! │  P A G E S   │                                      │
//! │  1 • ptop    │                                      │
//! │  2   lazygit │            the focused pane          │
//! │  3   cairn   │                                      │
//! │              │                                      │
//! │ P R O J E…   │                                      │
//! │ ▾ dirk       │                                      │
//! │   * mux core │                                      │
//! │   · shell    │                                      │
//! │   + workspace│                                      │
//! ├──────────────┴──────────────────────────────────────┤
//! │ ◆ dirk  ▊1 mux core ▏2 shell        2 spaces  14:22 │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! One event loop, one channel, three kinds of producer: the terminal reader,
//! one thread per pane, and a ticker. The loop blocks on a receive and drains
//! whatever else has queued before it draws, so a pane spewing output costs one
//! frame rather than one frame per write, and an idle dirk costs nothing at
//! all. There is no polling anywhere in this file.

mod agent;
mod api;
mod client;
mod config;
mod git;
mod glyph;
mod hit;
mod keys;
mod llm;
mod mux;
mod name;
mod notify;
mod server;
mod skill;
mod sound;
mod state;
mod theme;
mod tokens;
mod ui;
mod wire;

use config::Config;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use hit::{HitMap, Target};
use mux::{Dir, Ev, Focus, Pane, Session};
use ratatui::Frame;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::{Terminal, widgets::Widget};
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use theme::THEME;
use ui::nav::{Nav, Row};
use ui::picker::Picker;

const USAGE: &str = "\
Usage: dirk [OPTION]... [COMMAND]
Run a terminal multiplexer with a clickable project tree, static layouts, and
workspaces named from what the program inside them says it is doing.

With no command, attach to the session, starting it if it is not running.  The
session outlives the terminal it was started from: close this one and everything
in it keeps going.

Commands:
  attach                 attach to the session (the default)
  server                 be the session; started for you, not usually typed
  relay                  a session on stdin and stdout; what --remote runs

Asking a running session, from a shell or from inside a pane.  Answers are
JSON; `--current` means the pane you are in.

  workspace list|focus|create|rename|close
  pane      list|focus|split|read|send-keys|close
  layout    list|open
  agent     list|start|state|hooks
  session   info|list|reload|commands

Options:
      --session NAME     which session, default \"default\"
      --remote TARGET    attach to a session on TARGET, over ssh
      --no-session       one process, no session, ends with this terminal
      --skill            print what an agent needs to drive a session
  -h, --help             display this help and exit
  -V, --version          output version information and exit

dirk reads ~/.config/dirk/config.toml when it exists, and runs on built-in
defaults when it does not.  The prefix key is Ctrl-Space.  Press it and then
'd' to detach, leaving everything running, or 'q' to quit and end it.  Press
Ctrl-Space twice to send one through to whatever is inside.

Environment:
  DIRK_SSH               the program --remote reaches the far side with
  DIRK_REMOTE            the far-side dirk for --remote, default \"dirk\"
  DIRK_DEBUG             a file for a session\'s own stderr to land in

Report bugs to: <https://github.com/oddurs/dirk/issues>
";

const VERSION: &str = concat!(
    "dirk ",
    env!("CARGO_PKG_VERSION"),
    "\n",
    "Copyright (C) 2026 Oddur Sigurdsson\n",
    "License GPLv3+: GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>.\n",
    "This is free software: you are free to change and redistribute it.\n",
    "There is NO WARRANTY, to the extent permitted by law.\n",
    "\n",
    "Written by Oddur Sigurdsson.\n",
);

/// Print an answer, and stop quietly when nobody is reading it any more.
///
/// `print!` panics on a closed pipe, so `dirk session list | head -1` ends in a
/// backtrace where every other program on the system ends in silence.
/// Restoring the default disposition for SIGPIPE is the usual fix and is the
/// wrong one here: the server writes to panes and the client writes to an ssh
/// pipe, and both need EPIPE back as an error rather than as a signal.
fn say(text: &str) {
    use std::io::Write;
    let mut out = io::stdout();
    // Flushed here rather than left to the runtime, so the failure to write is
    // seen while there is still somewhere to report it.
    let wrote = out.write_all(text.as_bytes()).and_then(|()| out.flush());
    let Err(e) = wrote else { return };
    if e.kind() == io::ErrorKind::BrokenPipe {
        std::process::exit(0);
    }
    eprintln!("dirk: {e}");
    std::process::exit(1);
}

fn main() -> io::Result<()> {
    // Options are answered before the terminal is touched, so `dirk --version`
    // in a pipe behaves like any other program rather than briefly taking over
    // the screen. dirk takes at most one, and every one of them exits.
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The session a pane belongs to, which every pane is told. Without reading
    // it back, a command from inside one goes to `default` -- and pane ids are
    // per-session counters, so `--current` in one session can name a live pane
    // in another and type into a stranger's shell.
    let mut session = std::env::var("DIRK_SESSION").unwrap_or_else(|_| "default".to_string());
    let mut mode = Mode::Attach;
    let mut remote: Option<String> = None;

    // `--session` may come before a command, so the flags are read first and
    // whatever is left is the command.
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                say(USAGE);
                return Ok(());
            }
            "-V" | "--version" => {
                say(VERSION);
                return Ok(());
            }
            "--skill" => {
                say(&skill::text());
                return Ok(());
            }
            "server" => mode = Mode::Server,
            "relay" => mode = Mode::Relay,
            "--remote" => match rest.next() {
                Some(target) => remote = Some(target.clone()),
                None => {
                    eprintln!("dirk: --remote needs a target");
                    std::process::exit(1);
                }
            },
            "attach" => mode = Mode::Attach,
            "--no-session" => mode = Mode::Alone,
            "--session" => match rest.next() {
                Some(name) => session = name.clone(),
                None => {
                    eprintln!("dirk: --session needs a name");
                    std::process::exit(1);
                }
            },
            other if NOUNS.contains(&other) => break,
            other => {
                eprintln!("dirk: unrecognized option '{other}'");
                eprintln!("Try 'dirk --help' for more information.");
                std::process::exit(1);
            }
        }
    }

    // Before anything is answered, because the answers below are about this
    // machine. `dirk --remote box pane list` was reaching the local session and
    // saying so as though it had been asked about the remote one.
    if remote.is_some()
        && let Some(noun) = words(&args).first().filter(|w| NOUNS.contains(w))
    {
        eprintln!("dirk: --remote attaches; it does not carry a command");
        eprintln!(
            "Try: ssh TARGET dirk {}",
            words(&args)
                .iter()
                .skip_while(|w| *w != noun)
                .copied()
                .collect::<Vec<_>>()
                .join(" ")
        );
        std::process::exit(1);
    }

    if !server::valid_name(&session) {
        eprintln!("dirk: {session:?} is not a session name");
        std::process::exit(1);
    }
    let path = server::socket_path(&session);

    // A command is a noun and a verb. Answered by a running session, and never
    // by starting one: `dirk pane list` should say there is nothing to list
    // rather than conjure a session to list.
    // Answered without a running session, because they are about which ones
    // there are rather than about one of them.
    if command_args_are(&args, "session", "list") {
        for (name, running) in server::sessions() {
            if !running {
                // The socket outlived its server, which is the ordinary state
                // after a crash. Said plainly rather than hidden, because it is
                // the answer to "why can I not attach to that".
                say(&format!("{name}\tstale\n"));
                continue;
            }
            // Whether anyone is looking needs asking; only the session knows.
            // Once, not once per field: two questions can be answered either
            // side of a client attaching, and a line that says "running" with
            // the workspace count of an attached session is a line about two
            // different moments.
            let path = server::socket_path(&name);
            let req = wire::Request {
                cmd: "session.info".into(),
                args: Vec::new(),
            };
            let said = server::ask(&path, &req).ok();
            let field = |k: &str| said.as_ref().and_then(|r| r.result.get(k));
            let attached = field("attached").and_then(|v| v.as_bool()).unwrap_or(false);
            let spaces = field("workspaces").and_then(|v| v.as_u64()).unwrap_or(0);
            say(&format!(
                "{name}\t{}\t{spaces} {}\n",
                if attached { "attached" } else { "running" },
                if spaces == 1 { "space" } else { "spaces" }
            ));
        }
        return Ok(());
    }

    // Answered without a session, because installing a hook is something you do
    // before there is one -- and because the answer is a snippet to paste, which
    // is worse for having been through JSON on the way.
    if command_args_are(&args, "agent", "hooks") {
        let kind = words(&args).get(2).copied().unwrap_or("claude").to_string();
        say(&crate::agent::hooks(&kind));
        return Ok(());
    }

    let command_args: Vec<String> = args
        .iter()
        .skip_while(|a| !NOUNS.contains(&a.as_str()))
        .cloned()
        .collect();
    if let Some(req) = as_request(&command_args) {
        if !server::is_running(&path) {
            eprintln!("dirk: no session {session:?} is running");
            std::process::exit(1);
        }
        let reply = server::ask(&path, &req)?;
        match reply.ok {
            true => {
                say(&format!(
                    "{}\n",
                    serde_json::to_string_pretty(&reply.result).unwrap_or_default()
                ));
                return Ok(());
            }
            false => {
                eprintln!("dirk: {}", reply.error.unwrap_or_default());
                std::process::exit(1);
            }
        }
    }

    // Nothing local is involved: no session is started here, and the socket
    // this side computed is not the one that gets used.
    if let Some(target) = remote {
        return client::remote(&target, &session);
    }

    match mode {
        Mode::Server => serve(&session, &path),
        Mode::Relay => {
            // Started if it is not there, exactly as attaching would: the point
            // of a remote attach is that the far side behaves like the near one.
            if !server::is_running(&path) {
                server::spawn(&session, &path)?;
            }
            client::relay(&path)
        }
        Mode::Attach => {
            if !server::is_running(&path) {
                server::spawn(&session, &path)?;
            }
            client::attach(&path)
        }
        Mode::Alone => alone(),
    }
}

/// Is this `dirk <noun> <verb>`?
fn command_args_are(args: &[String], noun: &str, verb: &str) -> bool {
    let words = words(args);
    words.first() == Some(&noun) && words.get(1) == Some(&verb)
}

/// The command in `args`, with the options and their values taken out.
///
/// Dropping only the `-` words is not enough: `--session foo session list`
/// would leave `foo` in front of the command and match nothing.
fn words(args: &[String]) -> Vec<&str> {
    const TAKES_A_VALUE: &[&str] = &["--session", "--remote"];
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if arg.starts_with('-') {
            skip = TAKES_A_VALUE.contains(&arg.as_str());
            continue;
        }
        out.push(arg.as_str());
    }
    out
}

/// The nouns a session answers to.
const NOUNS: &[&str] = &["workspace", "pane", "layout", "agent", "session"];

/// Read `dirk pane split w1:p2 rows` as a request, or `None` if this is not one.
///
/// `--current` becomes the pane the caller is in, which is what makes the
/// surface usable from inside one without every caller having to look up its
/// own id first.
fn as_request(args: &[String]) -> Option<wire::Request> {
    let noun = args.first()?;
    if !NOUNS.contains(&noun.as_str()) {
        return None;
    }
    let verb = args.get(1)?;
    let rest = args
        .get(2..)
        .unwrap_or_default()
        .iter()
        .map(|a| match a.as_str() {
            "--current" => std::env::var("DIRK_PANE_ID").unwrap_or_else(|_| a.clone()),
            _ => a.clone(),
        })
        .collect();
    Some(wire::Request {
        cmd: format!("{noun}.{verb}"),
        args: rest,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Attach to the session, starting it if it is not there.
    Attach,
    /// Be the session. Started by the above; not usually typed.
    Server,
    /// One process, no session, dies with the terminal.
    Alone,
    /// A session socket on stdin and stdout. What ssh runs on the far side.
    Relay,
}

/// Be the session.
fn serve(session: &str, path: &std::path::Path) -> io::Result<()> {
    let listener = server::bind(path)?;
    // Inherited by every pane, so a command from inside one reaches the session
    // that holds it rather than the default.
    unsafe { std::env::set_var("DIRK_SESSION", session) };
    let cfg = Config::load();

    let (tx, rx) = mpsc::channel::<Ev>();
    spawn_ticker(tx.clone());
    server::listen(listener, tx.clone());

    // No terminal to ask, so a size until a client says otherwise.
    let size = ratatui::layout::Size {
        width: 80,
        height: 24,
    };
    let session_state = Session::new(&cfg, tx.clone());
    let mut app = App::new(cfg, session_state, size, tx);
    app.socket = Some(path.to_path_buf());
    app.socket_inode = server::inode(path).ok();
    app.session_name = Some(session.to_string());
    // What was here before, if anything was.
    if !app.restore() {
        app.bootstrap();
    }

    let ours = app.socket_inode;
    let result = app.serve(rx);

    // On the way out as well as on the tick. The shape is written a second at
    // a time, and ending the session is exactly when the last second has not
    // elapsed -- quitting promptly after a rename would otherwise lose it.
    //
    // Not when the session is empty: it ended because its last pane exited,
    // and writing that emptiness down would throw away the arrangement rather
    // than record one.
    if !app.session.is_empty() {
        app.persist();
    }
    // Only if it is still ours: another server may have bound this name while
    // we were shutting down, and removing its socket would orphan it.
    if ours.is_some_and(|i| server::reachable(path, i)) {
        let _ = std::fs::remove_file(path);
    }
    result
}

/// One process, no session. What dirk was before it had one.
fn alone() -> io::Result<()> {
    let cfg = Config::load();
    let (tx, rx) = mpsc::channel::<Ev>();
    spawn_input(tx.clone());
    spawn_ticker(tx.clone());

    let mut terminal = setup()?;
    let size = terminal.size()?;

    let session = Session::new(&cfg, tx.clone());
    let mut app = App::new(cfg, session, size, tx);
    app.bootstrap();

    let result = app.run(&mut terminal, rx);
    restore();
    result
}

// ── Terminal lifecycle ──────────────────────────────────────────────────

type Term = Terminal<CrosstermBackend<io::Stdout>>;

/// Narrower than this and a row cannot show a name, which is the only reason
/// the nav is there.
const MIN_NAV: u16 = 20;

fn setup() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;

    // A panic in raw mode leaves the terminal unusable and the backtrace
    // unreadable. Restore first, then let the default hook print.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        default(info);
    }));

    Terminal::new(CrosstermBackend::new(io::stdout()))
}

fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
}

fn spawn_input(tx: Sender<Ev>) {
    std::thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            if tx.send(Ev::Term(ev)).is_err() {
                return;
            }
        }
    });
}

/// One tick a second: the clock needs it, and so does naming — a title that
/// went quiet mid-debounce has no further output to wake the loop with.
fn spawn_ticker(tx: Sender<Ev>) {
    std::thread::spawn(move || {
        while tx.send(Ev::Tick).is_ok() {
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

/// Run one board's status command and take the first line of what it said.
///
/// One line, because a badge that wraps has already lost the argument -- and
/// what dirk does with the text is show it. It does not parse it: the moment
/// dirk starts understanding git's or cairn's output it owns their formats for
/// ever, and if you want `3↑ 2•` you write the script that prints `3↑ 2•`.
fn run_status(run: &[String], cap: Duration) -> Option<String> {
    let (program, args) = run.split_first()?;
    let mut child = std::process::Command::new(config::expand(program))
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // Given a deadline, because `output()` waits for as long as the child
    // feels like taking. A `git fetch` against a host that is not answering
    // would otherwise hold the one badge thread open for ever -- and with it
    // every other board, none of which would refresh again for the life of the
    // session.
    let until = Instant::now() + cap;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => return None,
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= until => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }

    let mut text = String::new();
    if let Some(out) = child.stdout.take() {
        use std::io::Read;
        // Bounded: what is wanted is one short line, and a command that decides
        // to print a gigabyte should not be able to make dirk hold it.
        let _ = out.take(64 * 1024).read_to_string(&mut text);
    }
    Some(text.lines().next().unwrap_or_default().trim().to_string())
}

/// What a board last said about itself.
struct Badge {
    /// The text, or nothing while it has not answered yet.
    text: Option<String>,
    /// When it last ran, successfully or not.
    at: Instant,
    /// Consecutive failures. A command that cannot run is a configuration
    /// problem, and retrying it every two seconds turns one mistake into a fan.
    fails: u32,
}

// ── App ─────────────────────────────────────────────────────────────────

struct App {
    cfg: Config,
    session: Session,
    hits: HitMap,
    /// The prefix key has been pressed and the next key is a command.
    prefix: bool,
    picker: Option<Picker>,
    sidebar: bool,
    /// A transient note in the rail, cleared on the next tick that finds it
    /// stale.
    status: String,
    status_at: Instant,
    /// Where panes live, kept so a pane can be spawned at the right size
    /// before it has ever been drawn.
    content: Rect,
    /// Kept so background work -- a git read, say -- can post its answer back
    /// to the one loop that owns the state.
    tx: Sender<Ev>,
    /// Where the nav is looking, and what it last drew. The rows are kept so a
    /// keystroke can act on the same list the pointer sees.
    nav: Nav,
    nav_rows: Vec<Row>,
    side: Rect,
    /// When the quit button was armed. Quitting ends every shell and agent in
    /// the session, and the button sits at the edge of the screen where a stray
    /// click is most likely, so it takes two.
    quit_armed: Option<Instant>,
    /// Which session this is, and what was last written down for it.
    session_name: Option<String>,
    written: Option<state::Saved>,
    /// The harnesses to recognise, resolved from the configuration.
    kinds: std::sync::Arc<Vec<crate::agent::Kind>>,
    /// The marks to draw with, resolved from the configuration.
    ///
    /// Here rather than in the renderer: it cannot change without a reload, and
    /// building twenty-two heap strings twice a frame -- once for the nav and
    /// once for the rail -- on a session where a pane is producing output is a
    /// few thousand allocations a second for a table that never moves.
    glyphs: glyph::Glyphs,
    /// What each board last reported, by name.
    badges: std::collections::HashMap<String, Badge>,
    /// One round of status commands at a time, for the same reason as `ps`.
    badging: bool,
    /// Which board was focused at the end of the last turn, by name, so one
    /// that does not keep its panes can be shut when you leave it.
    ///
    /// By name rather than by index for the same reason `merge_layouts` remaps
    /// focus by name: a reload can reorder the list, and an index kept across
    /// one closes a different board than the one you left.
    was: Option<String>,
    /// A saved session is there and unreadable, so this one does not write.
    readonly: bool,
    /// Whether the failure to save has been mentioned. Once is enough.
    complained: bool,
    /// Where this session's socket is, when it has one. Checked on the tick:
    /// a server whose socket has gone cannot be reached by anyone and should
    /// not keep holding a shell.
    socket: Option<std::path::PathBuf>,
    /// Which socket, by inode. A path can be reused by somebody else.
    socket_inode: Option<u64>,
    /// The client being drawn for, if one is attached. A session with nobody
    /// looking at it keeps running; that is the point of the whole milestone.
    view: Option<server::View>,
    /// True while a process-table sample is in flight.
    sampling: bool,
    /// True while the pointer is dragging the divider. Held as state because a
    /// drag is three events and only the first one lands on the divider.
    dragging: bool,
    quit: bool,
}

impl App {
    fn new(cfg: Config, session: Session, size: ratatui::layout::Size, tx: Sender<Ev>) -> Self {
        let kinds = std::sync::Arc::new(cfg.kinds());
        let glyphs = cfg.nav.glyphs();
        let width = size.width;
        let height = size.height;
        let sidebar_w = cfg.sidebar_width;
        Self {
            cfg,
            session,
            hits: HitMap::default(),
            prefix: false,
            picker: None,
            sidebar: true,
            status: String::new(),
            status_at: Instant::now(),
            tx,
            nav: Nav::default(),
            nav_rows: Vec::new(),
            side: Rect::ZERO,
            session_name: None,
            written: None,
            kinds,
            glyphs,
            badges: std::collections::HashMap::new(),
            badging: false,
            was: None,
            readonly: false,
            complained: false,
            socket: None,
            socket_inode: None,
            view: None,
            sampling: false,
            quit_armed: None,
            dragging: false,
            content: Rect {
                x: sidebar_w,
                y: 0,
                width: width.saturating_sub(sidebar_w),
                height: height.saturating_sub(1),
            },
            quit: false,
        }
    }

    /// Bring back the shape of a session that was here before.
    ///
    /// Not the panes: a pane is a process, and restoring a screenful of text
    /// with nothing behind it would be worse than an empty one, because it
    /// looks like something you can type into. What comes back is which
    /// projects were open, which workspaces were in them, and what they were
    /// called — the parts a human arranged.
    ///
    /// Returns false when there was nothing to bring back.
    fn restore(&mut self) -> bool {
        let Some(name) = self.session_name.clone() else {
            return false;
        };
        let saved = match state::read(&state::base(), &name) {
            state::Stored::Saved(saved) => saved,
            state::Stored::Fresh => return false,
            // Something is there that this dirk cannot read -- a newer format,
            // or a file it cannot open at all. Starting empty is right; writing
            // over it is not, because that file may be the only record of the
            // arrangement and the dirk that wrote it will want it back.
            state::Stored::Unreadable => {
                self.readonly = true;
                self.note("saved session not readable; it will not be written over");
                return false;
            }
        };
        let saved = state::prune(saved);
        if saved.projects.is_empty() {
            return false;
        }

        let (rows, cols) = (self.content.height, self.content.width);
        for project in &saved.projects {
            let p = self.session.open_project(&project.path);
            for want in &project.workspaces {
                if self.session.new_workspace(p, rows, cols).is_none() {
                    continue;
                }
                let Some(proj) = self.session.projects.get_mut(p) else {
                    continue;
                };
                let Some(ws) = proj.workspaces.last_mut() else {
                    continue;
                };
                // A name you wrote is still yours after a restart, and one
                // naming worked out is kept so the session reads as it read.
                ws.label = want.label.clone();
                ws.naming.held = want.held;
                ws.naming.applied = Some(want.label.clone());
            }
            // After the workspaces, not before: opening one expands the project
            // that holds it, so a collapsed project set up first is expanded
            // again on the way past.
            if let Some(proj) = self.session.projects.get_mut(p) {
                proj.expanded = project.expanded;
            }
        }
        // The top of the tree, which is where a session reads from. `refocus`
        // only guarantees somewhere valid, and somewhere valid after building a
        // tree bottom-up is the last workspace of the last project.
        if let Some(first) = self.session.first_workspace() {
            self.session.focus = first;
        }
        self.session.refocus();
        self.written = Some(state::current(&self.session));
        true
    }

    /// Write the shape down, when it has changed.
    ///
    /// On the tick rather than on every edit: the thing being saved changes a
    /// few times an hour and a write per keystroke would be a write per
    /// keystroke.
    fn persist(&mut self) {
        let Some(name) = self.session_name.clone() else {
            return;
        };
        if self.readonly {
            return;
        }
        let now = state::current(&self.session);
        if self
            .written
            .as_ref()
            .is_some_and(|was| !state::differs(was, &now))
        {
            return;
        }
        match state::save(&state::base(), &name, &now) {
            Ok(()) => self.written = Some(now),
            // Once, not once a second: the usual causes -- a full disk, a
            // read-only home -- do not clear up on their own, and a status line
            // repeating itself every tick is one you stop reading.
            Err(e) if !self.complained => {
                self.complained = true;
                self.note(&format!("cannot save this session: {e}"));
            }
            Err(_) => {}
        }
    }

    /// Open something so dirk does not start on an empty screen. The directory
    /// you launched from is the obvious guess and almost always the right one.
    fn bootstrap(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| config::home());
        let p = self.session.open_project(&cwd);
        let (rows, cols) = (self.content.height, self.content.width);
        self.session.new_workspace(p, rows, cols);
    }

    fn note(&mut self, msg: &str) {
        self.status = msg.to_string();
        self.status_at = Instant::now();
    }

    /// One turn of the loop: take everything queued, then settle the state.
    ///
    /// Returns false when there is nothing left to run for.
    fn turn(&mut self, rx: &Receiver<Ev>) -> bool {
        let Ok(ev) = rx.recv() else { return false };
        self.handle(ev);
        // Coalesce whatever else has already queued. A pane writing fast
        // produces one redraw, not one per write.
        while let Ok(next) = rx.try_recv() {
            self.handle(next);
        }
        // A board that does not keep its panes loses them when you look away.
        // Checked here rather than at every place focus can move, because focus
        // moves from keys, clicks, the API and a workspace closing under you.
        let now = match self.session.focus {
            Focus::Layout(i) => self.session.layouts.get(i).map(|l| l.def.name.clone()),
            Focus::Ws { .. } => None,
        };
        if let Some(left) = self.was.take().filter(|name| Some(name) != now.as_ref())
            && let Some(i) = self
                .session
                .layouts
                .iter()
                .position(|l| l.def.name == left && !l.def.keep)
        {
            self.session.close_layout(i);
        }
        self.was = now;

        let changes = self.session.update_states(Instant::now());
        self.announce(changes);
        !(self.quit || self.session.is_empty())
    }

    /// Serve whichever client is attached, for as long as the session lasts.
    fn serve(&mut self, rx: Receiver<Ev>) -> io::Result<()> {
        while self.turn(&rx) {
            // Taken out so the render can borrow the rest of `self`.
            let Some(mut view) = self.view.take() else {
                continue;
            };
            let _ = view.term.draw(|f| self.render(f));
            if view.flush().is_ok() {
                self.view = Some(view);
            }
        }
        if let Some(mut view) = self.view.take() {
            let _ = wire::send_json(&mut view.out, wire::Kind::Bye, &"the session ended");
        }
        Ok(())
    }

    fn run(&mut self, terminal: &mut Term, rx: Receiver<Ev>) -> io::Result<()> {
        terminal.draw(|f| self.render(f))?;

        while let Ok(ev) = rx.recv() {
            self.handle(ev);
            // Coalesce whatever else has already queued. A pane writing fast
            // produces one redraw, not one per write.
            while let Ok(next) = rx.try_recv() {
                self.handle(next);
            }
            // Once per frame, for the same reason drawing is: one `read` of a
            // busy pane produces an event, and working out every agent's state
            // locks each agent pane's terminal and reads its screen. Doing that
            // per chunk contends with the reader threads holding the same lock.
            let changes = self.session.update_states(Instant::now());
            self.announce(changes);
            if self.quit || self.session.is_empty() {
                return Ok(());
            }
            terminal.draw(|f| self.render(f))?;
        }
        Ok(())
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Term(Event::Key(k)) if k.kind != KeyEventKind::Release => self.on_key(k),
            Ev::Term(Event::Mouse(m)) => self.on_mouse(m),
            Ev::Term(Event::Resize(cols, rows)) => {
                if let Some(view) = &mut self.view {
                    let area = Rect::new(0, 0, cols.max(1), rows.max(1));
                    let _ = view.term.resize(area);
                    // The old contents are wrong at the new size, and ratatui
                    // would otherwise only send what changed against them.
                    let _ = view.repaint();
                }
            }
            Ev::Term(_) => {}
            Ev::Attach(view) => {
                if let Some(old) = self.view.take() {
                    // Told why, rather than simply going quiet.
                    let mut out = old.out;
                    let _ =
                        wire::send_json(&mut out, wire::Kind::Bye, &"taken over by another client");
                }
                let mut view = *view;
                // The client's terminal holds whatever was on it before, and
                // ratatui only sends what changed since its own last draw.
                let _ = view.repaint();
                self.view = Some(view);
            }
            Ev::Command(req, reply) => {
                let answer = self.ask(&req);
                let _ = reply.send(answer);
            }
            Ev::Detach(id) => {
                // Only if it is the client currently being drawn for. A client
                // that has been taken over closes its socket on the way out,
                // and an unnamed detach would clear the view of the one that
                // replaced it -- leaving the new client attached to a session
                // that had stopped drawing for it.
                if self.view.as_ref().is_some_and(|v| v.id == id) {
                    // The session does not end because nobody is watching it.
                    self.view = None;
                }
            }
            Ev::Output(id) => {
                self.session.touch(id);
                self.session.track_intents(&self.cfg.naming);
                self.rename_pass();
            }
            Ev::Git(answer) => self.session.apply_repo(answer),
            Ev::Badges(results) => {
                self.badging = false;
                let now = Instant::now();
                for (name, text) in results {
                    let fails = match &text {
                        Some(_) => 0,
                        None => self.badges.get(&name).map_or(1, |b| b.fails + 1),
                    };
                    self.badges.insert(
                        name,
                        Badge {
                            text,
                            at: now,
                            fails,
                        },
                    );
                }
            }
            Ev::Suggested { pane, intent } => self.session.apply_suggestion(pane, intent),
            Ev::Agents(reading) => {
                self.sampling = false;
                self.session.apply_agents(reading);
                self.session.name_agents(&self.cfg.naming);
            }
            Ev::Exited(id) => {
                self.session.reap(id);
                self.session.refocus();
            }
            Ev::Tick => {
                if let (Some(path), Some(ours)) = (&self.socket, self.socket_inode)
                    && !server::reachable(path, ours)
                {
                    // Unreachable: nothing can attach, and what is here would
                    // only be findable with `ps`.
                    self.quit = true;
                    return;
                }
                self.persist();
                self.read_badges();
                self.read_agents();
                self.read_repos();
                self.session.track_intents(&self.cfg.naming);
                self.ask_for_intents();
                self.rename_pass();
                if !self.status.is_empty()
                    && self.status_at.elapsed() > Duration::from_secs(3)
                    && !self.prefix
                {
                    self.status.clear();
                }
                // An armed button that stays armed is a trap of a different
                // shape: it disarms on its own.
                if self
                    .quit_armed
                    .is_some_and(|t| t.elapsed() > Duration::from_secs(3))
                {
                    self.quit_armed = None;
                }
            }
        }
    }

    /// Interrupt, but only for the transitions worth interrupting for.
    ///
    /// Blocked and finished-unseen, and neither for the workspace you are
    /// looking at — you can already see that one.
    fn announce(&mut self, changes: Vec<mux::session::Change>) {
        if !self.cfg.notify.enabled {
            return;
        }
        let floor = Duration::from_millis(self.cfg.notify.min_interval_ms);
        let now = Instant::now();
        for change in changes {
            if change.focused {
                continue;
            }
            let what = match change.to {
                agent::State::Blocked => "is waiting for you",
                agent::State::Done => "has finished",
                // Starting work and settling down are not interruptions.
                _ => continue,
            };
            if !self.session.may_notify(change.at, change.to, now, floor) {
                continue;
            }
            // A muted project still reaches the column, the counts and the
            // notification; what it does not do is make a noise.
            let quiet = self.project_of(change.at).is_some_and(|p| !p.sound);
            self.alert(
                change.to,
                format!("{} {what}", change.label),
                change.label.clone(),
                quiet,
            );
        }
    }

    /// Whether this workspace's project asked to be left alone.
    fn project_of(&self, at: Focus) -> Option<&config::ProjectDef> {
        let Focus::Ws { p, .. } = at else { return None };
        let path = self.session.projects.get(p)?.path.clone();
        self.cfg
            .projects
            .iter()
            .find(|d| config::expand(&d.path.to_string_lossy()) == path)
    }

    /// Interrupt somebody, wherever they are.
    ///
    /// Sent to the client when there is one, because that is the machine with
    /// the screen and the speakers -- attach over ssh and the session is on the
    /// build box while the human is at a laptop, and a notification delivered
    /// where nobody is sitting is one nobody gets. Performed here when there is
    /// no client, because a session you detached from is exactly the one you
    /// wanted to be told about.
    fn alert(&mut self, state: agent::State, title: String, label: String, quiet: bool) {
        let which = match state {
            agent::State::Blocked => sound::Alert::Blocked,
            agent::State::Done => sound::Alert::Done,
            _ => return,
        };
        if let Some(view) = self.view.as_mut() {
            let msg = wire::Alert {
                state: match quiet {
                    // Said rather than dropped: the notification is still owed,
                    // and only the noise was refused.
                    true => format!("{}-quiet", which.name()),
                    false => which.name().to_string(),
                },
                label,
            };
            let _ = wire::send_json(&mut view.out, wire::Kind::Alert, &msg);
            return;
        }
        notify::send(title, self.cfg.brand.name.clone());
        if !quiet {
            sound::play(&self.cfg.sound, which);
        }
    }

    /// Do what was asked, or say why not.
    ///
    /// Reads are answered by `api::read` and never touch the seen rule: asking
    /// about a workspace is not looking at one, and without that a status line
    /// polling the session would clear every notification it exists to show.
    fn ask(&mut self, req: &wire::Request) -> wire::Reply {
        use wire::Reply;
        if let Some(answer) = api::read(&self.session, &req.cmd, &req.args) {
            return answer;
        }
        let area = self.content;
        let arg = |n: usize| req.args.get(n).cloned().unwrap_or_default();

        match req.cmd.as_str() {
            "session.reload" => match Config::reload(&mut self.cfg, &mut self.session) {
                Ok(said) => {
                    // Rebuilt here, which is the only place it can change.
                    self.glyphs = self.cfg.nav.glyphs();
                    self.kinds = std::sync::Arc::new(self.cfg.kinds());
                    Reply::ok(serde_json::json!({
                        "reloaded": true,
                        // Whatever the file was wrong about. Empty is the usual
                        // answer and the only one worth not reading.
                        "notes": said,
                    }))
                }
                Err(e) => Reply::err(e),
            },

            "session.info" => Reply::ok(serde_json::json!({
                "workspaces": self.session.flat().len(),
                "layouts": self.session.layouts.len(),
                "attached": self.view.is_some(),
                "version": env!("CARGO_PKG_VERSION"),
            })),

            "workspace.focus" => match api::target_workspace(&self.session, &arg(0)) {
                Some((p, w)) => {
                    self.session.focus = Focus::Ws { p, w };
                    Reply::ok(serde_json::json!({ "focused": arg(0) }))
                }
                None => Reply::err("no such workspace"),
            },

            "workspace.create" => {
                let path = if arg(0).is_empty() {
                    std::env::current_dir().unwrap_or_else(|_| config::home())
                } else {
                    config::expand(&arg(0))
                };
                if !path.is_dir() {
                    return Reply::err("no such directory");
                }
                let p = self.session.open_project(&path);
                // Saved and put back, as `pane.split` does: a background
                // command should not pull the attached human away from what
                // they were doing, nor resize their panes doing it.
                let was = self.session.focus;
                let made = self.session.new_workspace(p, area.height, area.width);
                self.session.focus = was;
                self.session.refocus();
                match made {
                    Some(()) => {
                        let w = self.session.projects[p].workspaces.len() - 1;
                        let id = self.session.projects[p].workspaces[w].id;
                        Reply::ok(serde_json::json!({ "workspace": api::workspace_id(id) }))
                    }
                    None => Reply::err("could not start a shell there"),
                }
            }

            "workspace.rename" => match api::target_workspace(&self.session, &arg(0)) {
                Some((p, w)) => {
                    let name = req.args[1..].join(" ");
                    if name.trim().is_empty() {
                        return Reply::err("a name, or nothing to hand it back");
                    }
                    let Some(ws) = self.session.workspace_mut(p, w) else {
                        return Reply::err("no such workspace");
                    };
                    ws.label = name.clone();
                    // Named from outside is named by a human: naming stands
                    // down until the hold is released.
                    ws.naming.held = true;
                    ws.naming.applied = Some(name.clone());
                    Reply::ok(serde_json::json!({ "label": name }))
                }
                None => Reply::err("no such workspace"),
            },

            "workspace.close" => match api::target_workspace(&self.session, &arg(0)) {
                Some((p, w)) => {
                    let ids: Vec<_> = self
                        .session
                        .workspace(p, w)
                        .map(|ws| ws.panes.iter().map(|x| x.id).collect())
                        .unwrap_or_default();
                    for id in ids {
                        if let Some(ws) = self.session.workspace_mut(p, w)
                            && let Some(pane) = ws.pane_mut(id)
                        {
                            pane.close();
                        }
                    }
                    Reply::ok(serde_json::json!({ "closed": arg(0) }))
                }
                None => Reply::err("no such workspace"),
            },

            "pane.focus" => match api::target_pane(&self.session, &arg(0)) {
                Some(id) => match api::locate(&self.session, id) {
                    Some((p, w)) => {
                        self.session.focus = Focus::Ws { p, w };
                        if let Some(ws) = self.session.workspace_mut(p, w) {
                            ws.focus = id;
                        }
                        Reply::ok(serde_json::json!({ "focused": arg(0) }))
                    }
                    None => Reply::err("no such pane"),
                },
                None => Reply::err("no such pane"),
            },

            "pane.split" => {
                let Some(id) = api::target_pane(&self.session, &arg(0)) else {
                    return Reply::err("no such pane");
                };
                let Some((p, w)) = api::locate(&self.session, id) else {
                    return Reply::err("no such pane");
                };
                let dir = if arg(1).eq_ignore_ascii_case("rows") {
                    Dir::Rows
                } else {
                    Dir::Cols
                };

                // Split where asked, not wherever the human happens to be
                // looking. A caller that meant "here" said so with an id.
                let was = self.session.focus;
                self.session.focus = Focus::Ws { p, w };
                if let Some(ws) = self.session.workspace_mut(p, w) {
                    ws.focus = id;
                }
                self.session.split(dir, area.height, area.width);
                let new = self.session.workspace(p, w).map(|ws| ws.focus);
                self.session.focus = was;

                match new {
                    Some(pane) => {
                        let ws_id = self.session.workspace(p, w).map(|x| x.id).unwrap_or(0);
                        Reply::ok(serde_json::json!({ "pane": api::pane_id(ws_id, pane) }))
                    }
                    None => Reply::err("could not split"),
                }
            }

            "pane.send-keys" => {
                let Some(id) = api::target_pane(&self.session, &arg(0)) else {
                    return Reply::err("no such pane");
                };
                let text = req.args[1..].join(" ");
                match self.session.write_to(id, text.as_bytes()) {
                    true => Reply::ok(serde_json::json!({ "sent": text.len() })),
                    false => Reply::err("no such pane"),
                }
            }

            "pane.close" => {
                let Some(id) = api::target_pane(&self.session, &arg(0)) else {
                    return Reply::err("no such pane");
                };
                match self.session.close_pane(id) {
                    true => Reply::ok(serde_json::json!({ "closed": arg(0) })),
                    false => Reply::err("no such pane"),
                }
            }

            "layout.open" => {
                match self
                    .session
                    .layouts
                    .iter()
                    .position(|l| l.def.name == arg(0))
                {
                    Some(i) => {
                        self.session.open_layout(i, area);
                        Reply::ok(serde_json::json!({ "opened": arg(0) }))
                    }
                    None => Reply::err("no such layout"),
                }
            }

            // Rank one: the agent saying what it is doing, which is a fact
            // where everything else dirk has is a guess.
            "agent.state" => {
                let Some(state) = crate::agent::State::named(&arg(0)) else {
                    return Reply::err("no such state");
                };
                // No target means the workspace you are looking at. A target
                // that was given and did not resolve is an error, not an
                // invitation to pick one: `--current` outside a pane arrives
                // here as the literal string, and a hook firing just after its
                // workspace closed would otherwise land its state -- and its
                // notification, and its noise -- on whatever you happen to be
                // looking at instead.
                let target = match arg(1).is_empty() {
                    true => match self.session.focus {
                        Focus::Ws { p, w } => Some((p, w)),
                        Focus::Layout(_) => None,
                    },
                    false => api::target_workspace(&self.session, &arg(1)),
                };
                let Some((p, w)) = target else {
                    return Reply::err("no such workspace");
                };
                let Some(ws) = self.session.workspace_mut(p, w) else {
                    return Reply::err("no such workspace");
                };
                ws.reported = Some((state, Instant::now()));
                Reply::ok(serde_json::json!({
                    "workspace": api::workspace_id(ws.id),
                    "state": state.glyph_name(),
                }))
            }

            "agent.start" => {
                let Some(kind) = self.kinds.iter().find(|k| k.name == arg(0)).cloned() else {
                    return Reply::err(format!("no such agent: {}", arg(0)));
                };
                if kind.command.is_empty() {
                    return Reply::err(format!("{} has no command to start it with", kind.name));
                }
                let Some(pane) = api::target_pane(&self.session, &arg(1)) else {
                    return Reply::err("no such pane");
                };
                match self.start_agent(pane, &kind, false) {
                    Ok(id) => Reply::ok(serde_json::json!({
                        "started": kind.name,
                        "pane": id,
                    })),
                    Err(why) => Reply::err(why),
                }
            }

            "agent.hooks" => match self.kinds.iter().find(|k| k.name == arg(0)) {
                Some(kind) => Reply::ok(serde_json::json!({
                    "kind": kind.name,
                    "install": crate::agent::hooks(&kind.name),
                })),
                None => Reply::err(format!("no such agent: {}", arg(0))),
            },

            other => Reply::err(format!("no such command: {other}")),
        }
    }

    /// Start an agent where you are looking, making somewhere for it if the
    /// pane you are in is busy.
    ///
    /// A workspace exists in order to hold an agent -- it is why the naming
    /// policy is built around what the agent says it is doing -- so this is one
    /// keystroke rather than a new workspace and then a command typed into it.
    fn new_agent(&mut self) {
        let Focus::Ws { p, w } = self.session.focus else {
            return self.note("no project focused");
        };
        let path = self.session.projects.get(p).map(|x| x.path.clone());
        let Some(name) = path.as_deref().and_then(|d| self.cfg.agent_for(d)) else {
            return self.note("no agent configured: set default_agent");
        };
        let Some(kind) = self.kinds.iter().find(|k| k.name == name).cloned() else {
            return self.note(&format!("no such agent: {name}"));
        };

        // The focused pane if it is free, and a new workspace if it is not.
        // Typing into somebody's editor is not recoverable by apologising.
        let free = self
            .session
            .workspace(p, w)
            .and_then(|ws| ws.active_pane())
            .is_some_and(|pane| pane.occupant.available());
        let mut ours = false;
        if !free {
            let (rows, cols) = (self.content.height, self.content.width);
            if self.session.new_workspace(p, rows, cols).is_none() {
                return self.note("could not make a workspace");
            }
            ours = true;
        }
        let Some(pane) = self
            .session
            .workspace(
                p,
                match self.session.focus {
                    Focus::Ws { w, .. } => w,
                    Focus::Layout(_) => w,
                },
            )
            .map(|ws| ws.focus)
        else {
            return self.note("nowhere to start it");
        };
        match self.start_agent(pane, &kind, ours) {
            Ok(_) => self.note(&format!("starting {name}")),
            Err(why) => self.note(&why),
        }
    }

    /// Type an agent's command into a pane that is free.
    ///
    /// Refuses one that is not at a prompt rather than typing over what is
    /// there: the whole reason `available` exists in the API is that writing
    /// into somebody's editor is not recoverable by apologising.
    ///
    /// The pane is marked `starting` straight away, because the gap between
    /// the command being typed and the process appearing in the table is
    /// exactly the window where "did that work" is worth an answer.
    fn start_agent(
        &mut self,
        pane: crate::mux::PaneId,
        kind: &crate::agent::Kind,
        ours: bool,
    ) -> Result<String, String> {
        let Some((p, w)) = api::locate(&self.session, pane) else {
            return Err("no such pane".into());
        };
        let Some(ws) = self.session.workspace(p, w) else {
            return Err("no such pane".into());
        };
        let Some(target) = ws.pane(pane) else {
            return Err("no such pane".into());
        };
        // A pane dirk made a moment ago is not somebody else's, and it has not
        // been sampled yet: a fresh one reads as `Unknown` until the next `ps`
        // round, which is up to a second away. Checking availability there
        // meant the "make somewhere for it" path could never succeed -- it
        // refused the workspace it had just created.
        if !ours && !target.occupant.available() {
            return Err(format!(
                "that pane is busy: {}",
                match &target.occupant {
                    crate::agent::Occupant::Agent(k) => k.name.clone(),
                    crate::agent::Occupant::Program(p) => p.clone(),
                    _ => "not at a prompt".into(),
                }
            ));
        }
        let id = api::pane_id(ws.id, pane);
        let line = format!("{}\r", kind.command.join(" "));
        if !self.session.write_to(pane, line.as_bytes()) {
            return Err("no such pane".into());
        }
        if let Some(ws) = self.session.workspace_mut(p, w) {
            ws.reported = Some((crate::agent::State::Starting, Instant::now()));
        }
        Ok(id)
    }

    /// Ask the second source about panes whose title says nothing.
    ///
    /// Off the drawing thread like every other external call, and the answer
    /// arrives as an event. A request that hangs must not be able to stop dirk
    /// redrawing, which is the whole reason for the shape.
    fn ask_for_intents(&mut self) {
        let cfg = &self.cfg.naming.sources.llm;
        if !self.cfg.naming.enabled || !cfg.enabled {
            return;
        }
        for (pane, screen) in self.session.wants_intent(cfg, Instant::now()) {
            let cfg = cfg.clone();
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                if let Some(intent) = crate::llm::suggest(&cfg, &screen) {
                    let _ = tx.send(Ev::Suggested { pane, intent });
                }
            });
        }
    }

    /// Ask what is running in each pane.
    ///
    /// The foreground process groups are read here — a `tcgetpgrp` each, too
    /// cheap to be worth a thread. Turning them into names needs the process
    /// table, which does not belong on the drawing thread: `ps` on a loaded
    /// machine is slow, and a slow `ps` must not be able to stop dirk redrawing.
    /// Ask each board that is due what it has to say.
    ///
    /// One round at a time and never on the drawing thread, like every other
    /// external call. A board with no `status` is not asked anything, which is
    /// what keeps the default configuration exactly as cheap as it was.
    fn read_badges(&mut self) {
        if self.badging {
            return;
        }
        let now = Instant::now();
        let due: Vec<(String, Vec<String>, Duration)> = self
            .cfg
            .layouts
            .iter()
            .filter_map(|l| {
                let status = l.status.as_ref()?;
                if status.run.is_empty() {
                    return None;
                }
                let wait = match self.badges.get(&l.name) {
                    // Backing off: doubling to a cap, so a command that cannot
                    // run costs one attempt a minute rather than thirty.
                    // Saturating, because `Duration: Mul<u32>` panics on
                    // overflow and the interval comes out of a file.
                    Some(b) if b.fails > 0 => status
                        .interval()
                        .saturating_mul(2u32.saturating_pow(b.fails.min(5))),
                    Some(_) => status.interval(),
                    None => return Some((l.name.clone(), status.run.clone(), status.cap())),
                };
                let last = self.badges.get(&l.name)?.at;
                (now.duration_since(last) >= wait)
                    .then(|| (l.name.clone(), status.run.clone(), status.cap()))
            })
            .collect();
        if due.is_empty() {
            return;
        }

        self.badging = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            for (name, run, cap) in due {
                out.push((name, run_status(&run, cap)));
            }
            let _ = tx.send(Ev::Badges(out));
        });
    }

    fn read_agents(&mut self) {
        // One sample at a time. Without this, a `ps` slower than the tick --
        // which is the loaded machine this is written to survive -- accumulates
        // threads and processes without bound, and lets an old reading land
        // after a newer one and overwrite it.
        if self.sampling {
            return;
        }
        let panes = self.session.foregrounds();
        if panes.is_empty() {
            return;
        }
        self.sampling = true;
        let tx = self.tx.clone();
        // Resolved once and carried into the thread: the table is configuration
        // now, and reading it here rather than there keeps `session reload`
        // from changing what a sample in flight is comparing against.
        let kinds = std::sync::Arc::clone(&self.kinds);
        std::thread::spawn(move || {
            let table = crate::agent::table();
            let panes = panes
                .into_iter()
                .map(|(id, pgid)| {
                    let occupant = match pgid {
                        // Nothing in the foreground: the pane is holding
                        // nothing, and that is news.
                        None => Some(crate::agent::Occupant::Unknown),
                        // A group that is not in the table ended between the
                        // pgid being read and `ps` running. That is not news.
                        Some(pgid) => table
                            .get(&pgid)
                            .map(|proc| crate::agent::identify(proc, &kinds)),
                    };
                    (id, occupant)
                })
                .collect();
            let _ = tx.send(Ev::Agents(crate::agent::Reading { panes }));
        });
    }

    /// Ask git about any project whose answer has gone stale.
    ///
    /// One thread per read, and the answer comes back as an event. A `git` that
    /// has gone to a network remote or is waiting on an index lock would
    /// otherwise stall the whole program for the sake of a caption.
    fn read_repos(&mut self) {
        for dir in self.session.stale_repos(Instant::now()) {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let repo = crate::git::read(&dir);
                let _ = tx.send(Ev::Git(crate::git::Answer { dir, repo }));
            });
        }
    }

    /// Ask `name.rs` about every workspace. Cheap: it is a few string tests
    /// per workspace, and it short-circuits on the first one that fails.
    fn rename_pass(&mut self) {
        let now = Instant::now();
        for p in 0..self.session.projects.len() {
            let repo = self.session.projects[p].name.clone();
            // Without this, a workspace labelled with its own branch name looks
            // hand-written and naming backs off from it permanently.
            let branch = self.session.projects[p]
                .repo
                .as_ref()
                .map(|r| r.branch.clone())
                .unwrap_or_default();
            for w in 0..self.session.projects[p].workspaces.len() {
                let ws = &self.session.projects[p].workspaces[w];
                // The title first, always. The second source is for the pane
                // that has none, and its candidate goes through this same
                // policy with no exemptions.
                let title = ws
                    .active_pane()
                    .and_then(|x| x.title())
                    .or_else(|| ws.suggested.clone());
                let panes = ws.panes.len();
                let current = ws.label.clone();
                let blocked = ws.state == agent::State::Blocked;

                let ws = &mut self.session.projects[p].workspaces[w];
                if !self.cfg.naming.targets.workspace {
                    continue;
                }
                if let name::Decision::Rename(intent) = name::decide(
                    &self.cfg.naming,
                    &mut ws.naming,
                    &current,
                    title.as_deref(),
                    &repo,
                    &branch,
                    panes,
                    blocked,
                    now,
                ) {
                    // The decision is what the intent should be; the template
                    // is how it is arranged. Rendered after, so the policy
                    // compares intents with intents rather than with whatever
                    // a template happened to produce.
                    let mut tokens = self.session.tokens(p, w);
                    tokens.set("intent", intent.clone());
                    tokens.set("intent-slug", name::slugify(&intent, name::AGENT_NAME_MAX));
                    let label = tokens::render(&self.cfg.naming.templates.workspace, &tokens);
                    let ws = &mut self.session.projects[p].workspaces[w];
                    let label = if label.is_empty() { intent } else { label };
                    // Recorded as written, so "did a human change this" compares
                    // the live label against the label rather than against the
                    // intent it was rendered from.
                    ws.naming.applied = Some(label.clone());
                    ws.label = label;
                }
            }
        }
    }

    // ── Geometry ────────────────────────────────────────────────────────

    fn areas(&self, full: Rect) -> (Rect, Rect, Rect) {
        let rail = Rect {
            x: full.x,
            y: full.bottom().saturating_sub(1),
            width: full.width,
            height: 1,
        };
        let body = Rect {
            height: full.height.saturating_sub(1),
            ..full
        };
        let sw = if self.sidebar {
            self.cfg.sidebar_width.min(body.width / 2)
        } else {
            0
        };
        let side = Rect { width: sw, ..body };
        let content = Rect {
            x: body.x + sw,
            width: body.width - sw,
            ..body
        };
        (side, content, rail)
    }

    /// The rects of the panes currently on screen, in the order they are drawn.
    fn visible_rects(&self) -> Vec<Rect> {
        match self.session.focus {
            Focus::Layout(i) => match self.session.layouts.get(i).and_then(|l| l.ws.as_ref()) {
                Some(ws) => ws.rects(self.content).into_iter().map(|(_, r)| r).collect(),
                None => Vec::new(),
            },
            Focus::Ws { p, w } => match self.session.workspace(p, w) {
                Some(ws) => ws.rects(self.content).into_iter().map(|(_, r)| r).collect(),
                None => Vec::new(),
            },
        }
    }

    fn visible_pane_mut(&mut self, index: usize) -> Option<&mut Pane> {
        match self.session.focus {
            Focus::Layout(i) => {
                let ws = self.session.layouts.get_mut(i)?.ws.as_mut()?;
                let id = *ws.tree.leaves().get(index)?;
                ws.pane_mut(id)
            }
            Focus::Ws { p, w } => {
                let ws = self.session.workspace_mut(p, w)?;
                let id = *ws.tree.leaves().get(index)?;
                ws.pane_mut(id)
            }
        }
    }

    // ── Render ──────────────────────────────────────────────────────────

    fn render(&mut self, f: &mut Frame) {
        self.hits.clear();
        let full = f.area();
        let (side, content, rail) = self.areas(full);
        self.content = content;
        self.session.resize_visible(content);

        self.side = side;
        let buf = f.buffer_mut();
        if side.width > 0 {
            let badges = |name: &str| {
                self.badges
                    .get(name)
                    // A dash is an answer: the board was asked and could not
                    // reply, which is not the same as one nobody asked.
                    .map(|b| {
                        b.text
                            .clone()
                            .unwrap_or_else(|| self.glyphs.text(glyph::G::Absent).to_string())
                    })
            };
            self.nav_rows = ui::nav::render(
                buf,
                side,
                &ui::nav::Frame {
                    cfg: &self.cfg,
                    glyphs: &self.glyphs,
                    session: &self.session,
                    badges: &badges,
                },
                &mut self.nav,
                &mut self.hits,
            );
        }
        draw_content(buf, content, &self.glyphs, &self.session, &mut self.hits);

        let clock = chrono::Local::now().format("%H:%M").to_string();
        let status = if self.prefix { "prefix" } else { &self.status };
        ui::rail::render(
            buf,
            rail,
            &self.cfg,
            &self.glyphs,
            &self.session,
            &ui::rail::Now {
                clock: &clock,
                note: status,
                quit_armed: self.quit_armed.is_some(),
            },
            &mut self.hits,
        );

        if let Some(p) = &self.picker {
            ui::picker::render(
                buf,
                Rect {
                    height: full.height.saturating_sub(1),
                    ..full
                },
                p,
                &mut self.hits,
            );
            return;
        }

        // The cursor belongs to the focused pane, and only when that pane is
        // showing one.
        // The cursor goes where the screen was drawn, which for a labelled pane
        // is below the rule rather than at the top of its rectangle.
        if let (Some(pane), Some(rect)) = (self.session.active_pane(), self.focused_rect())
            && let Ok(t) = pane.term.lock()
            && let Some(pos) = ui::pane::cursor(
                t.screen(),
                mux::session::content_of(pane.label.is_some(), rect),
            )
        {
            f.set_cursor_position(pos);
        }
    }

    fn focused_rect(&self) -> Option<Rect> {
        let rects = self.visible_rects();
        match self.session.focus {
            Focus::Layout(i) => {
                let ws = self.session.layouts.get(i)?.ws.as_ref()?;
                let at = ws.tree.leaves().iter().position(|&id| id == ws.focus)?;
                rects.get(at).copied()
            }
            Focus::Ws { p, w } => {
                let ws = self.session.workspace(p, w)?;
                let at = ws.tree.leaves().iter().position(|&id| id == ws.focus)?;
                rects.get(at).copied()
            }
        }
    }

    /// Do the thing the user pointed at, however they pointed at it.
    ///
    /// Clicking a row and pressing Enter on it arrive here with the same value,
    /// which is the only reason the two cannot drift apart.
    fn act(&mut self, target: Target) {
        // Doing anything else is a change of mind.
        if !matches!(target, Target::Quit) {
            self.quit_armed = None;
        }
        let area = self.content;
        match target {
            Target::Layout(i) => self.session.open_layout(i, area),
            Target::ProjectFold(p) => {
                if let Some(proj) = self.session.projects.get_mut(p) {
                    proj.expanded = !proj.expanded;
                }
                // Folding is not going anywhere, so the nav keeps the keyboard.
                return;
            }
            Target::Workspace { p, w } => {
                // The rows a keystroke acts on were built for the last frame,
                // and a workspace can close between frames. Focusing one that
                // is gone leaves no focused pane at all.
                if self.session.workspace(p, w).is_none() {
                    return;
                }
                // Clicking a workspace that is already focused and has several
                // panes opens it, which is the only thing left for that click
                // to mean.
                let expand = self.session.focus == Focus::Ws { p, w }
                    && self
                        .session
                        .workspace(p, w)
                        .is_some_and(|ws| ws.panes.len() > 1);
                if expand && let Some(ws) = self.session.workspace_mut(p, w) {
                    ws.expanded = !ws.expanded;
                    return;
                }
                self.session.focus = Focus::Ws { p, w };
            }
            Target::NavPane { p, w, index } => {
                self.session.focus = Focus::Ws { p, w };
                if let Some(ws) = self.session.workspace_mut(p, w)
                    && let Some(&id) = ws.tree.leaves().get(index)
                {
                    ws.focus = id;
                }
            }
            Target::NewAgent => {
                self.new_agent();
                return;
            }
            Target::NewWorkspace(p) => {
                self.session.new_workspace(p, area.height, area.width);
            }
            Target::OpenProject => {
                self.open_picker();
                return;
            }
            Target::PickerRow(i) => {
                let path = self
                    .picker
                    .as_ref()
                    .and_then(|p| p.path(i))
                    .map(|p| p.to_path_buf());
                self.picker = None;
                if let Some(path) = path {
                    self.open(&path);
                }
            }
            Target::Attention(state) => {
                // The oldest first: what has been waiting longest is what to
                // look at first. No early return -- this moves you, so it falls
                // through to handing the keyboard back like every other target
                // that does.
                if let Some(at) = self.session.oldest_in(state) {
                    self.session.focus = at;
                }
            }
            Target::Detach => {
                self.detach();
                return;
            }
            Target::Quit => {
                match self.quit_armed {
                    Some(_) => self.quit = true,
                    None => {
                        self.quit_armed = Some(Instant::now());
                        self.note("click again to quit");
                    }
                }
                return;
            }
            Target::Pane { .. } => return,
        }
        // Anything that moved you somewhere hands the keyboard back, because
        // that is what you went there for.
        self.nav.active = false;
    }

    // ── Keys ────────────────────────────────────────────────────────────

    fn on_key(&mut self, k: KeyEvent) {
        if self.picker.is_some() {
            return self.picker_key(k);
        }
        if self.nav.active && !self.prefix {
            return self.nav_key(k);
        }
        if self.prefix {
            self.prefix = false;
            self.status.clear();
            return self.command(k);
        }
        // Ctrl-Space is the prefix. Ctrl-a and Ctrl-b are both load-bearing in
        // every shell line editor; Ctrl-Space is NUL, which nothing sends on
        // purpose.
        if k.code == KeyCode::Char(' ') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.prefix = true;
            self.note("prefix");
            return;
        }
        self.send_key(k);
    }

    /// Keys while the nav holds them. No prefix: a nav that needs one before
    /// every `j` is not a nav.
    fn nav_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.nav.active = false,
            KeyCode::Char('j') | KeyCode::Down => self.nav.step(&self.nav_rows, 1),
            KeyCode::Char('k') | KeyCode::Up => self.nav.step(&self.nav_rows, -1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(t) = self.nav.selection(&self.nav_rows).and_then(Row::target) {
                    self.act(t);
                }
            }
            KeyCode::Char('n') => {
                if let Focus::Ws { p, .. } = self.session.focus {
                    self.act(Target::NewWorkspace(p));
                }
            }
            KeyCode::Char('o') => self.act(Target::OpenProject),
            _ => {}
        }
    }

    fn send_key(&mut self, k: KeyEvent) {
        let Some(pane) = self.session.active_pane_mut() else {
            return;
        };
        if pane.dead {
            return;
        }
        let app_cursor = pane
            .term
            .lock()
            .map(|t| t.screen().application_cursor())
            .unwrap_or(false);
        if let Some(bytes) = keys::encode(k, app_cursor) {
            pane.write(&bytes);
        }
    }

    /// Leave, and let the session carry on.
    ///
    /// The mechanism was already here and nothing called it: the server handles
    /// a client going away, and the client handles being told to go. This is
    /// the second one, said on purpose rather than because a terminal closed.
    fn detach(&mut self) {
        if self.session_name.is_none() {
            // `--no-session`: there is nothing behind this process to leave
            // running, so detaching would only be quitting with a nicer word.
            self.note("no session to detach from");
            return;
        }
        match self.view.take() {
            Some(mut view) => {
                let _ = wire::send_json(&mut view.out, wire::Kind::Bye, &"detached");
            }
            None => self.note("nothing attached"),
        }
    }

    fn command(&mut self, k: KeyEvent) {
        let (rows, cols) = (self.content.height, self.content.width);
        match k.code {
            // Pressing the prefix twice sends a literal one through.
            KeyCode::Char(' ') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(p) = self.session.active_pane_mut() {
                    p.write(&[0]);
                }
            }
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('d') => self.detach(),
            KeyCode::Char('n') => {
                if let Focus::Ws { p, .. } = self.session.focus {
                    self.session.new_workspace(p, rows, cols);
                } else {
                    self.note("no project focused");
                }
            }
            KeyCode::Char('o') => self.open_picker(),
            KeyCode::Char('a') => self.new_agent(),
            KeyCode::Char('x') => self.session.close_focused(),
            KeyCode::Char('u') => {
                if self.session.release_hold() {
                    self.note("naming released");
                } else {
                    self.note("not held");
                }
            }
            KeyCode::Char('r') => {
                if !self.session.restart_focused(self.content) {
                    self.note("nothing to restart");
                }
            }
            KeyCode::Char('|') | KeyCode::Char('v') => self.session.split(Dir::Cols, rows, cols),
            KeyCode::Char('-') | KeyCode::Char('s') => self.session.split(Dir::Rows, rows, cols),
            KeyCode::Char('b') => self.sidebar = !self.sidebar,
            KeyCode::Char('w') => {
                self.nav.active = !self.nav.active;
                if self.nav.active {
                    // Giving the keyboard to something that is not on screen
                    // looks exactly like a freeze, and the prefix cannot be
                    // reached from inside nav mode to undo it.
                    self.sidebar = true;
                    // Start where the eye already is.
                    self.nav.sync(&self.nav_rows, self.session.focus);
                }
            }
            KeyCode::Char(';') => self.session.cycle_pane(),
            KeyCode::Tab | KeyCode::Char('j') | KeyCode::Down => self.session.step_workspace(1),
            KeyCode::BackTab | KeyCode::Char('k') | KeyCode::Up => self.session.step_workspace(-1),
            KeyCode::Char(c) => {
                if let Some(i) = self
                    .session
                    .layouts
                    .iter()
                    .position(|l| l.def.key == Some(c))
                {
                    self.session.open_layout(i, self.content);
                }
            }
            _ => {}
        }
    }

    // ── Picker ──────────────────────────────────────────────────────────

    fn open_picker(&mut self) {
        self.picker = Some(Picker::new(&self.cfg.projects_root));
    }

    fn picker_key(&mut self, k: KeyEvent) {
        let Some(p) = self.picker.as_mut() else {
            return;
        };
        match k.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Enter => {
                if let Some(i) = p.matches().get(p.selected).map(|(i, _)| *i) {
                    self.act(Target::PickerRow(i));
                }
            }
            KeyCode::Backspace => {
                p.query.pop();
                p.selected = 0;
            }
            KeyCode::Down | KeyCode::Tab => p.move_by(1),
            KeyCode::Up | KeyCode::BackTab => p.move_by(-1),
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                p.query.push(c);
                p.selected = 0;
            }
            _ => {}
        }
    }

    /// Open a project, adding a workspace only if it has none — clicking a
    /// project you already have open should take you there, not pile up shells.
    fn open(&mut self, path: &std::path::Path) {
        let (rows, cols) = (self.content.height, self.content.width);
        let p = self.session.open_project(path);
        match self.session.projects[p].workspaces.is_empty() {
            true => {
                self.session.new_workspace(p, rows, cols);
            }
            false => self.session.focus = Focus::Ws { p, w: 0 },
        }
    }

    // ── Mouse ───────────────────────────────────────────────────────────

    fn on_mouse(&mut self, m: MouseEvent) {
        let target = self.hits.at(m.column, m.row);

        // The divider, before anything else: a drag that started on it owns
        // every event until the button comes up, wherever the pointer has got
        // to by then.
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if self.on_divider(m.column) => {
                self.dragging = true;
                return;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                // Only a floor here. `Ord::clamp` panics when min exceeds max,
                // and on a terminal narrower than forty columns half of it is
                // under the floor -- so a clamp would abort rather than
                // resize. The ceiling is `areas`' job, and it already does it.
                self.cfg.sidebar_width = m.column.max(MIN_NAV);
                return;
            }
            MouseEventKind::Up(_) if self.dragging => {
                self.dragging = false;
                return;
            }
            _ => {}
        }

        // The wheel over the nav scrolls the nav, not whatever pane is behind
        // the pointer.
        if m.column < self.side.right() && self.side.width > 0 {
            let delta: isize = match m.kind {
                MouseEventKind::ScrollDown => 1,
                MouseEventKind::ScrollUp => -1,
                _ => 0,
            };
            if delta != 0 {
                // The section under the pointer, not the nav as a whole.
                self.nav.scroll_by(m.row, delta);
                return;
            }
        }

        if let MouseEventKind::Down(MouseButton::Left) = m.kind
            && let Some(t) = target
            && !matches!(t, Target::Pane { .. })
        {
            return self.act(t);
        }

        // Anything else in the content area belongs to the pane under it.
        if self.picker.is_none() {
            self.send_mouse(&m);
        }
    }

    /// The divider is the nav's last column. Two columns wide as a target,
    /// because one is very hard to hit.
    fn on_divider(&self, column: u16) -> bool {
        self.sidebar
            && self.side.width > 0
            && column + 1 >= self.side.right()
            && column <= self.side.right()
    }

    fn send_mouse(&mut self, m: &MouseEvent) {
        let rects = self.visible_rects();
        let Some(index) = rects.iter().position(|r| {
            m.column >= r.x && m.column < r.right() && m.row >= r.y && m.row < r.bottom()
        }) else {
            return;
        };
        let r = rects[index];

        // Clicking a pane focuses it, whether or not the program inside also
        // wants the click.
        if let MouseEventKind::Down(_) = m.kind
            && let Some(ws) = self.session.focused_workspace_mut()
            && let Some(&id) = ws.tree.leaves().get(index)
        {
            ws.focus = id;
        }

        let Some(pane) = self.visible_pane_mut(index) else {
            return;
        };

        // A labelled pane's terminal starts below the rule, so the event has to
        // be measured from there. Measuring from the rectangle would put every
        // click one row low -- selecting the line above the one you pointed at.
        let inner = mux::session::content_of(pane.label.is_some(), r);
        if m.row < inner.y {
            // The rule itself is chrome. Focusing was the whole of that click.
            return;
        }
        let (col, row) = (m.column - inner.x, m.row - inner.y);

        let bytes = pane
            .term
            .lock()
            .ok()
            .and_then(|t| ui::pane::encode_mouse(t.screen(), m, col, row));
        if let Some(b) = bytes {
            pane.write(&b);
        }
    }
}

/// Draw whatever the focus points at. Free function so the session can be
/// borrowed while the hit map is written.
///
/// A layout and a project workspace draw identically, because a layout is a
/// workspace. The only thing this function decides is which one.
fn draw_content(
    buf: &mut Buffer,
    area: Rect,
    g: &glyph::Glyphs,
    session: &Session,
    hits: &mut HitMap,
) {
    let ws = match session.focus {
        Focus::Layout(i) => match session.layouts.get(i).and_then(|l| l.ws.as_ref()) {
            Some(ws) => ws,
            None => return empty(buf, area, "layout is not open"),
        },
        Focus::Ws { p, w } => match session.workspace(p, w) {
            Some(ws) => ws,
            None => return empty(buf, area, "nothing open — press ctrl-space o"),
        },
    };

    for (i, (id, r)) in ws.rects(area).into_iter().enumerate() {
        let Some(pane) = ws.pane(id) else { continue };

        // A labelled pane gives its top row to a rule carrying the label, which
        // is how a five-pane dashboard says which panel is which.
        let inner = mux::session::content_of(pane.label.is_some(), r);
        if let Some(label) = &pane.label {
            rule(
                buf,
                Rect { height: 1, ..r },
                g,
                label,
                pane.exit.as_deref(),
                id == ws.focus,
            );
        }
        if let Ok(t) = pane.term.lock() {
            // A stopped pane keeps its last output, dimmed. The output is
            // usually why the panel was there.
            ui::pane::blit(t.screen(), inner, buf, id != ws.focus || pane.dead);
        }
        hits.push(r, Target::Pane { index: i });
    }
}

/// `─ brief ─────────────`. A rule rather than a border: three sides of a box
/// only repeat what the neighbouring pane's own edge already says.
fn rule(
    buf: &mut Buffer,
    area: Rect,
    g: &glyph::Glyphs,
    label: &str,
    exit: Option<&str>,
    focused: bool,
) {
    ui::fill(buf, area, THEME.panel());

    // The tail is measured first: the label's budget has to know about it, or a
    // long label is drawn and then written over halfway through a word.
    let tail = exit.map(|e| format!(" {e} · r restarts "));
    let tail_w = tail
        .as_ref()
        .map_or(0, |t| ui::cells(t).min(area.width.saturating_sub(4)));

    let mut x = area.x;
    // The focused pane is marked on its own rule. Without it, a dashboard whose
    // panels have all stopped is uniformly dim and nothing says which one `r`
    // would bring back.
    let (lead, lead_style) = if focused {
        (g.text(glyph::G::BarFocused), THEME.working())
    } else {
        (g.text(glyph::G::Rule), THEME.rule_strong())
    };
    x += ui::write_str(buf, x, area.y, lead, lead_style, area.width);
    x += ui::write_str(buf, x, area.y, " ", lead_style, area.width);

    let left = area
        .width
        .saturating_sub(x - area.x)
        .saturating_sub(tail_w + 2) as usize;
    let style = match (exit.is_some(), focused) {
        (false, _) => THEME.title(),
        (true, true) => THEME.text(),
        (true, false) => THEME.dim(),
    };
    x += ui::write_str(
        buf,
        x,
        area.y,
        &ui::elide(label, left, g.text(glyph::G::Ellipsis)),
        style,
        area.width,
    );
    x += ui::write_str(buf, x, area.y, " ", THEME.rule_strong(), area.width);

    for c in x..area.right().saturating_sub(tail_w) {
        ui::write_str(
            buf,
            c,
            area.y,
            g.text(glyph::G::Rule),
            THEME.rule_strong(),
            1,
        );
    }
    // Elided rather than dropped: a narrow stopped panel showing frozen output
    // and no explanation is the worst version of this.
    if let Some(t) = tail
        && tail_w > 0
    {
        let text = ui::elide(&t, tail_w as usize, g.text(glyph::G::Ellipsis));
        ui::write_str(
            buf,
            area.right() - tail_w,
            area.y,
            &text,
            THEME.warn(),
            tail_w,
        );
    }
}

fn empty(buf: &mut Buffer, area: Rect, msg: &str) {
    ratatui::widgets::Clear.render(area, buf);
    if area.height > 1 {
        let x = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
        ui::write_str(
            buf,
            x,
            area.y + area.height / 2,
            msg,
            THEME.faint(),
            area.width,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_name_is_not_mistaken_for_the_command() {
        // Dropping only the `-` words leaves the value behind, and
        // `--session foo session list` then reads as a command called "foo".
        let args = |s: &str| -> Vec<String> { s.split(' ').map(String::from).collect() };
        assert!(command_args_are(&args("session list"), "session", "list"));
        assert!(command_args_are(
            &args("--session foo session list"),
            "session",
            "list"
        ));
        assert!(!command_args_are(&args("--session foo"), "session", "list"));
        assert_eq!(
            words(&args("--session foo pane read w1:p2")),
            ["pane", "read", "w1:p2"]
        );
    }
}
