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

mod action;
mod agent;
mod api;
mod client;
mod clipboard;
mod complete;
mod config;
mod copy;
mod find;
mod git;
mod glyph;
mod hit;
mod hook;
mod keys;
mod llm;
mod mux;
mod name;
mod notify;
mod palette;
mod server;
mod skill;
mod sound;
mod state;
mod theme;
mod tokens;
mod ui;
mod wait;
mod wire;

use api::NOUNS;
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
  pane      list|focus|split|read|run|send-text|send-keys|close
  layout    list|open
  agent     list|rules|explain|start|state|prompt|wait|hooks
  worktree  list|add|remove
  tab       list|new|focus|rename|close
  session   info|list|reload|commands|notify|quit|prune
  api       schema

Options:
      --session NAME     which session, default \"default\"
      --remote TARGET    attach to a session on TARGET, over ssh
      --no-session       one process, no session, ends with this terminal
      --skill            print what an agent needs to drive a session
      --keys             list every action and the key it answers to
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
            "--keys" => {
                let cfg = Config::load();
                let keys = cfg.keys();
                let mut out = String::from("The prefix is Ctrl-Space, then:\n\n");
                for a in crate::action::Action::ALL {
                    out.push_str(&format!(
                        "  {:<4} {:<22} {}\n",
                        keys.key(*a).unwrap_or("--"),
                        a.name(),
                        a.title()
                    ));
                }
                out.push_str(
                    "\nRebind one in config.toml:\n\n  [keys]\n  \"session.quit\" = \"Q\"\n",
                );
                say(&out);
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
            // Not a noun: it is about setting the shell up rather than about a
            // session, so it is answered below without one.
            "completion" => break,
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
    // Answered without a session for the same reason `session list` is: it is
    // about which sessions there are rather than about one of them, and the
    // ones it removes are by definition not answering.
    if command_args_are(&args, "session", "prune") {
        let mut gone = 0usize;
        for (name, running) in server::sessions() {
            if running {
                continue;
            }
            // Asked again rather than trusting the listing: a session can come
            // up between the two, and removing a socket somebody is listening
            // on orphans a running server nobody can reach.
            let path = server::socket_path(&name);
            if server::is_running(&path) {
                continue;
            }
            if std::fs::remove_file(&path).is_ok() {
                gone += 1;
                say(&format!("{name}\tremoved\n"));
            }
        }
        if gone == 0 {
            say("nothing to remove\n");
        }
        return Ok(());
    }

    // Answered without a session because it is about the surface rather than
    // about any session of it, and because a caller generating a client wants
    // it before there is one to ask.
    if command_args_are(&args, "api", "schema") {
        say(&format!(
            "{}\n",
            serde_json::to_string_pretty(&api::schema()).unwrap_or_default()
        ));
        return Ok(());
    }

    // Answered without a session for the same reason as the schema: the shell
    // is being set up, not driven.
    if words(&args).first() == Some(&"completion") {
        let shell = words(&args).get(1).copied().unwrap_or_default();
        match complete::script(shell) {
            Some(text) => say(&text),
            None => {
                eprintln!("dirk: no completions for {shell:?}");
                eprintln!("Try one of: {}", complete::SHELLS.join(", "));
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Answered without a session, because the point of it is that a screen
    // somebody captured can be run through the same rule as a live one. That
    // is how a wrong detection becomes a test case instead of a bug report
    // with a screenshot in it.
    if command_args_are(&args, "agent", "explain")
        && let Some(file) = value_of(&args, "--file")
    {
        let cfg = Config::load();
        let kinds = cfg.kinds();
        let name = value_of(&args, "--agent").unwrap_or_default();
        let Some(kind) = kinds.iter().find(|k| k.name == name) else {
            eprintln!("dirk: --file needs --agent, naming a harness dirk knows");
            eprintln!(
                "Try one of: {}",
                kinds
                    .iter()
                    .map(|k| k.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            std::process::exit(1);
        };
        let window = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) => {
                // Structured, like every other failure: the caller is a script
                // feeding it captured screens.
                say(&format!(
                    "{}\n",
                    serde_json::json!({ "ok": false, "error": format!("{file}: {e}") })
                ));
                std::process::exit(1);
            }
        };
        let found = crate::agent::examine(kind, &window);
        say(&format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({
                "agent": kind.name,
                "rules": kind.from.name(),
                "blocked": found.blocked(),
                "why_not": found.why_not(),
                "screen": {
                    "marker": found.marker,
                    "line": found.line,
                    "menu_required": found.needs_menu,
                    "menu_found": found.menu,
                },
            }))
            .unwrap_or_default()
        ));
        return Ok(());
    }

    // Answered without a session, because installing a hook is something you do
    // before there is one -- and because a snippet to paste is worse for having
    // been through JSON on the way.
    if command_args_are(&args, "agent", "hooks") {
        let words = words(&args);
        let (verb, kind) = match words.get(2).copied() {
            Some(v @ ("install" | "uninstall" | "status")) => {
                (v, words.get(3).copied().unwrap_or("claude"))
            }
            other => ("print", other.unwrap_or("claude")),
        };
        let said = match verb {
            "print" => {
                say(&crate::agent::hooks(kind));
                return Ok(());
            }
            "status" => {
                // Every harness dirk can install for, not only the one asked
                // about: the question behind this is "am I running on rank one
                // anywhere", and one line per harness answers it.
                let cfg = Config::load();
                let mut out = String::new();
                for k in cfg.kinds() {
                    let Some(path) = hook::path(&k.name) else {
                        continue;
                    };
                    out.push_str(&format!(
                        "{:<14} {:<10} {}\n",
                        k.name,
                        hook::state(&k.name).name(),
                        path.display()
                    ));
                }
                if out.is_empty() {
                    out.push_str("dirk installs hooks for no harness it knows about\n");
                }
                say(&out);
                return Ok(());
            }
            "install" => hook::install(kind),
            _ => hook::uninstall(kind),
        };
        match said {
            Ok(line) => {
                say(&format!("{line}\n"));
                return Ok(());
            }
            Err(why) => {
                eprintln!("dirk: {why}");
                std::process::exit(1);
            }
        }
    }

    // Not a request and a reply: what comes back is a stream, and the client
    // owns the terminal for as long as it lasts. So it is intercepted here
    // rather than answered by `ask`.
    if command_args_are(&args, "pane", "attach") {
        let words = words(&args);
        let Some(target) = words.get(2) else {
            eprintln!("dirk: pane attach needs a pane");
            std::process::exit(1);
        };
        let target = match *target {
            "--current" => std::env::var("DIRK_PANE_ID").unwrap_or_else(|_| target.to_string()),
            other => other.to_string(),
        };
        if !server::is_running(&path) {
            eprintln!("dirk: no session {session:?} is running");
            std::process::exit(1);
        }
        let takeover = args.iter().any(|a| a == "--takeover");
        return match client::watch(&path, &target, takeover) {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("dirk: {e}");
                std::process::exit(1);
            }
        };
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

/// The value of a `--name value` or `--name=value` option, if it was given.
fn value_of(args: &[String], name: &str) -> Option<String> {
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if let Some(v) = arg.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
        if arg == name {
            return rest.next().cloned();
        }
    }
    None
}

/// The command in `args`, with the options and their values taken out.
///
/// Dropping only the `-` words is not enough: `--session foo session list`
/// would leave `foo` in front of the command and match nothing.
fn words(args: &[String]) -> Vec<&str> {
    const TAKES_A_VALUE: &[&str] = &["--session", "--remote", "--file", "--agent"];
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
    let mut rest: Vec<String> = args
        .get(2..)
        .unwrap_or_default()
        .iter()
        .filter(|a| *a != "--hook")
        .map(|a| match a.as_str() {
            "--current" => std::env::var("DIRK_PANE_ID").unwrap_or_else(|_| a.clone()),
            _ => a.clone(),
        })
        .collect();
    // `--hook` says the harness has piped its own payload in. dirk reads what
    // it understands out of it -- today, the harness's name for the
    // conversation -- and the rest of the command is unchanged, so a payload
    // that says nothing costs nothing.
    if args.iter().any(|a| a == "--hook") {
        let mut payload = String::new();
        use std::io::Read;
        let _ = io::stdin().read_to_string(&mut payload);
        if let Some(id) = agent::session_of(&payload) {
            rest.push("--session".into());
            rest.push(id);
        }
    }
    Some(wire::Request {
        cmd: format!("{noun}.{verb}"),
        args: rest,
    })
}

/// What a command turns out to be, before the ordinary answer path sees it.
enum Begin {
    /// Answer it the usual way.
    Ordinary,
    /// Already done, and here is what to say. For a command that acts before it
    /// decides whether to wait — running it twice would act twice.
    Answered(serde_json::Value),
    /// Its answer does not exist yet.
    Waiting(wait::What),
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

    // No terminal to ask, so a size until a client says otherwise. It is
    // configuration rather than a constant because a session can be driven
    // with nobody attached at all, and then this is not a placeholder -- it is
    // the size every pane the caller makes will have.
    let size = ratatui::layout::Size {
        width: cfg.server.headless_cols,
        height: cfg.server.headless_rows,
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

    let mut terminal = setup(cfg.ui.mouse)?;
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

fn setup(mouse: bool) -> io::Result<Term> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    // All or nothing. dirk captures so that it can decide per pane whether the
    // program inside wanted the click; declining to capture hands the outer
    // terminal its own selection back, and everything a click reaches has a
    // key.
    if mouse {
        execute!(out, EnableMouseCapture)?;
    }

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
            if tx.send(Ev::Term(None, ev)).is_err() {
                return;
            }
        }
    });
}

/// What this machine is called, for a title that says which one it is.
///
/// Trimmed to the first label: a session on `build.example.com` is on `build`,
/// and the rest is a domain nobody is choosing a window by.
fn hostname() -> String {
    let mut buf = [0i8; 256];
    // SAFETY: the buffer is ours and the length is its own.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr(), buf.len()) } == 0;
    if !ok {
        return String::new();
    }
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| *b as u8)
        .collect();
    String::from_utf8_lossy(&bytes)
        .split('.')
        .next()
        .unwrap_or_default()
        .to_string()
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

/// A question dirk is waiting on one line of text for.
///
/// Small on purpose. The project picker is the right shape for choosing from a
/// list that exists; this is for the case where the answer is a thing you are
/// about to make, and there is nothing to list.
struct Ask {
    /// What is being asked, shown before the text.
    what: &'static str,
    text: String,
    then: Then,
}

/// What to do with the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    Worktree,
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
    /// Which key runs what, after the configuration has had its say.
    keys: action::Keys,
    /// Which client dirk is currently acting for, if any. Set around handling
    /// that client's input, so anything that has to answer "who asked" can.
    acting: Option<u64>,
    /// The content rectangle every pane is sized to: the smallest among the
    /// clients watching. `None` when nobody is, or when there is no server.
    shared: Option<Rect>,
    /// Where `content` goes back to when the last client leaves.
    ///
    /// Kept rather than recomputed because `content` has by then been written
    /// over with the departed client's size, and a pane made after that would
    /// inherit the geometry of a terminal nobody is looking at.
    headless: Rect,
    /// What each board last reported, by name.
    badges: std::collections::HashMap<String, Badge>,
    /// One round of status commands at a time, for the same reason as `ps`.
    badging: bool,
    /// The last title posted, so an unchanged one is not sent again.
    ///
    /// A title rewritten every tick makes some terminals flash their tab, and a
    /// window manager that logs title changes logs one a second.
    titled: Option<String>,
    /// Terminals attached to one pane each, with no interface around them.
    watchers: Vec<server::Watcher>,
    /// Workspaces whose agent is to be started again on the conversation it was
    /// having, once somebody is here to watch.
    to_resume: Vec<(usize, u64)>,
    /// Questions whose answer does not exist yet.
    ///
    /// Settled at the end of every turn, which is after the states have been
    /// worked out — so a wait ends on the turn the thing it was waiting for
    /// happened, rather than on the next tick after it.
    waits: Vec<wait::Held>,
    /// Selecting, when that is what is happening.
    copy: Option<copy::Mode>,
    /// Searching, when that is.
    find: Option<find::Find>,
    /// A question that wants one line of text back.
    asking: Option<Ask>,
    /// Everything dirk can do, when somebody has asked.
    palette: Option<palette::Palette>,
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
    /// The clients being drawn for. A session with nobody looking at it keeps
    /// running; that is the point of the milestone this came from.
    ///
    /// Several of them, because the case two clients exist for is a laptop and
    /// a monitor showing different parts of the same session -- which means
    /// each carries its own focus, its own nav and its own size.
    views: Vec<server::View>,
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
        let keys = cfg.keys();
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
            keys,
            acting: None,
            shared: None,
            headless: Rect {
                x: sidebar_w,
                y: 0,
                width: width.saturating_sub(sidebar_w),
                height: height.saturating_sub(1),
            },
            badges: std::collections::HashMap::new(),
            badging: false,
            titled: None,
            watchers: Vec::new(),
            to_resume: Vec::new(),
            waits: Vec::new(),
            copy: None,
            find: None,
            asking: None,
            palette: None,
            was: None,
            readonly: false,
            complained: false,
            socket: None,
            socket_inode: None,
            views: Vec::new(),
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
        let mut pending: Vec<(usize, u64)> = Vec::new();
        for project in &saved.projects {
            // Only when there is nothing else to go on. Each space opens its
            // own checkout below, and opening the repository proper as well
            // would leave a checkout with no spaces in it -- polled by git
            // every fifteen seconds for a directory nobody asked for.
            if project.workspaces.is_empty() {
                self.session.open_project(&project.path);
            }
            for want in &project.workspaces {
                // In the checkout it was in. A space that was in a worktree
                // goes back to that worktree; without this every one of them
                // would come back in the repository proper, which is the one
                // arrangement nobody chose.
                let at = match want.at.as_os_str().is_empty() {
                    true => project.path.clone(),
                    false => want.at.clone(),
                };
                let p = self.session.open_project(&at);
                if self.session.new_workspace_at(p, &at, rows, cols).is_none() {
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
                // Remembered rather than started. Twelve agents resumed on a
                // server nobody may ever attach to is twelve model sessions
                // nobody asked for, so this waits for somebody to arrive --
                // which is also when there is a real terminal size to draw at.
                if let (Some(kind), Some(id)) = (&want.agent, &want.agent_session) {
                    ws.agent_session = Some((kind.clone(), id.clone()));
                    pending.push((p, ws.id));
                }
                // The first tab exists already; the rest are made, and all of
                // them take back the names they had.
                for (i, name) in want.tabs.iter().enumerate() {
                    if i > 0 && self.session.new_tab(rows, cols).is_none() {
                        break;
                    }
                    let Some(ws) = self
                        .session
                        .projects
                        .get_mut(p)
                        .and_then(|x| x.workspaces.last_mut())
                    else {
                        break;
                    };
                    if let Some(tab) = ws.tabs.get_mut(i) {
                        // A name that is only the position is not a name: it
                        // would freeze what the number happened to be.
                        if *name != (i + 1).to_string() {
                            tab.label = name.clone();
                        }
                    }
                }
            }
            // After the workspaces, not before: opening one expands the project
            // that holds it, so a collapsed project set up first is expanded
            // again on the way past. Found by key, because a project restored
            // only from worktrees was never opened by its own path.
            let key = self.session.projects.iter().position(|x| {
                x.path == project.path || x.workspaces.iter().any(|w| w.at == project.path)
            });
            if let Some(proj) = key.and_then(|i| self.session.projects.get_mut(i)) {
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
        self.to_resume = pending;
        true
    }

    /// Start the agents a restore remembered, on the conversations they were
    /// having.
    ///
    /// Run when the first client arrives rather than at restore. Resuming
    /// twelve agents on a server nobody may ever attach to is twelve model
    /// sessions nobody asked for — and this is also the first moment there is a
    /// real terminal size to draw them at.
    fn resume_agents(&mut self) {
        if !self.cfg.session.resume_agents {
            self.to_resume.clear();
            return;
        }
        for (p, ws_id) in std::mem::take(&mut self.to_resume) {
            let Some(w) = self
                .session
                .projects
                .get(p)
                .and_then(|proj| proj.workspaces.iter().position(|x| x.id == ws_id))
            else {
                continue;
            };
            let Some((kind, id)) = self
                .session
                .workspace(p, w)
                .and_then(|ws| ws.agent_session.clone())
            else {
                continue;
            };
            // A reference dirk cannot use is not an error. Every restored pane
            // used to be a shell, and one that stays a shell has lost nothing
            // it had a moment ago.
            let Some(kind) = self.kinds.iter().find(|k| k.name == kind).cloned() else {
                continue;
            };
            if kind.resume.is_empty() {
                continue;
            }
            let Some(pane) = self.session.workspace(p, w).map(|ws| ws.focus()) else {
                continue;
            };
            let line = kind
                .resume
                .iter()
                .map(|word| word.replace("{session}", &id))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = self.session.write_to(pane, format!("{line}\r").as_bytes());
            if let Some(ws) = self.session.workspace_mut(p, w) {
                ws.reported = Some((crate::agent::State::Starting, Instant::now()));
            }
        }
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
        // Woken by the nearest deadline as well as by events, when anything is
        // waiting on one. Otherwise a `--timeout 250` would be answered on the
        // next tick, which is a second away — and a timeout that is four times
        // what was asked for is not a timeout.
        let ev = match self.waits.iter().filter_map(|h| h.deadline).min() {
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(ev) => Some(ev),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return false,
            },
            None => match rx.recv() {
                Ok(ev) => Some(ev),
                Err(_) => return false,
            },
        };
        if let Some(ev) = ev {
            self.handle(ev);
        }
        // Coalesce whatever else has already queued. A pane writing fast
        // produces one redraw, not one per write.
        while let Ok(next) = rx.try_recv() {
            self.handle(next);
        }
        self.after_events();
        !(self.quit || self.session.is_empty())
    }

    /// Do something as one viewer, with their focus and their nav.
    ///
    /// Focus stopped being a property of the session the moment there could be
    /// two clients, and `session.focus` is read in fifty places that have no
    /// business knowing that. So it is a register: loaded from the viewer who
    /// is acting, and stored back when they are done.
    ///
    /// The alternative is threading a viewer through every one of those places,
    /// which is fifty chances to thread the wrong one.
    fn as_viewer(&mut self, from: Option<u64>, act: impl FnOnce(&mut Self)) {
        let at = from.and_then(|id| self.views.iter().position(|v| v.id == id));
        if let Some(i) = at {
            self.session.focus = self.views[i].focus;
            self.nav = std::mem::take(&mut self.views[i].nav);
            self.nav_rows = std::mem::take(&mut self.views[i].rows);
        }
        self.acting = from;
        act(self);
        self.acting = None;
        if let Some(i) = at.filter(|i| *i < self.views.len()) {
            self.views[i].focus = self.session.focus;
            self.views[i].nav = std::mem::take(&mut self.nav);
            self.views[i].rows = std::mem::take(&mut self.nav_rows);
        }
    }

    fn viewer_mut(&mut self, from: Option<u64>) -> Option<&mut server::View> {
        match from {
            Some(id) => self.views.iter_mut().find(|v| v.id == id),
            None => self.views.first_mut(),
        }
    }

    /// Size every pane to the narrowest screen showing it.
    ///
    /// What tmux does, and the only answer that is not a lie to somebody: a
    /// pane drawn wider than the smallest client can show would be cut off
    /// there, and one drawn to the largest would waste the rest.
    fn resize_panes(&mut self) {
        let Some(area) = self
            .views
            .iter()
            .map(|v| v.size())
            .reduce(|a, b| Rect::new(0, 0, a.width.min(b.width), a.height.min(b.height)))
            .filter(|a| a.width > 0 && a.height > 0)
        else {
            // Nobody is watching. The panes keep the size they had rather than
            // being resized to nothing, so what is in them survives until
            // somebody comes back.
            return;
        };
        let (_, content, _) = self.areas(area);
        self.shared = Some(content);
        self.session.resize_visible(content);
    }

    /// Serve whichever client is attached, for as long as the session lasts.
    fn serve(&mut self, rx: Receiver<Ev>) -> io::Result<()> {
        while self.turn(&rx) {
            // Each in turn, each with its own focus and its own nav, because
            // two people looking at one session are looking at two things.
            // Taken out so the render can borrow the rest of `self`.
            let mut views = std::mem::take(&mut self.views);
            views.retain_mut(|view| {
                self.session.focus = view.focus;
                self.nav = std::mem::take(&mut view.nav);
                let _ = view.term.draw(|f| self.render(f));
                view.nav = std::mem::take(&mut self.nav);
                view.rows = std::mem::take(&mut self.nav_rows);
                // A client that has stopped taking frames is gone; keeping it
                // would block the loop the next time round.
                view.flush().is_ok()
            });
            let before = self.views.len() + views.len();
            self.views = views;
            // A client that stopped taking frames has gone, and the panes are
            // sized to the smallest one still watching.
            if self.views.len() != before {
                self.resize_panes();
            }
        }
        for view in std::mem::take(&mut self.views) {
            let mut out = view.out;
            let _ = wire::send_json(&mut out, wire::Kind::Bye, &"the session ended");
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
            self.after_events();
            if self.quit || self.session.is_empty() {
                return Ok(());
            }
            terminal.draw(|f| self.render(f))?;
        }
        Ok(())
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Term(from, Event::Key(k)) if k.kind != KeyEventKind::Release => {
                self.as_viewer(from, |app| app.on_key(k));
            }
            // Dropped rather than merely not asked for. The setting means dirk
            // does not use the mouse, and that has to stay true when something
            // else has turned reporting on -- an outer multiplexer, or a
            // terminal that reports without being asked.
            Ev::Term(_, Event::Mouse(_)) if !self.cfg.ui.mouse => {}
            Ev::Term(from, Event::Mouse(m)) => {
                self.as_viewer(from, |app| app.on_mouse(m));
            }
            Ev::Term(from, Event::Resize(cols, rows)) => {
                if let Some(view) = self.viewer_mut(from) {
                    view.resized(cols, rows);
                }
                // A pane is as wide as the narrowest screen showing it, which
                // is what tmux does and the only answer that is not a lie to
                // somebody.
                self.resize_panes();
            }
            Ev::Term(..) => {}
            Ev::Attach(view) => {
                let mut view = *view;
                // Somewhere real to look. A client arriving at `Layout(0)`
                // would open on a board nobody asked for.
                view.focus = self
                    .session
                    .first_workspace()
                    .unwrap_or(crate::mux::Focus::Layout(0));
                // The client's terminal holds whatever was on it before, and
                // ratatui only sends what changed since its own last draw.
                let _ = view.repaint();
                self.views.push(view);
                self.resize_panes();
                // Now that somebody is here, and now that there is a terminal
                // size that belongs to a screen rather than to a fallback.
                self.resume_agents();
            }
            Ev::Command(req, reply) => {
                // Questions whose answer does not exist yet are put aside
                // rather than answered. The caller's socket thread is already
                // blocked on the other end of `reply`, so holding it is the
                // whole of what "wait" means here.
                match self.begin(&req) {
                    Ok(Begin::Answered(value)) => {
                        let _ = reply.send(wire::Reply::ok(value));
                        return;
                    }
                    Ok(Begin::Waiting(what)) => {
                        let (_, opts) = wait::options(&req.args);
                        match wait::deadline(&opts) {
                            Ok(deadline) => {
                                self.waits.push(wait::Held {
                                    back: reply,
                                    what,
                                    deadline,
                                });
                                // Asked and already true is answered now. A
                                // caller that waits for `idle` on an agent
                                // sitting idle should not wait at all.
                                self.settle();
                            }
                            Err(why) => {
                                let _ = reply.send(wire::Reply::err(why));
                            }
                        }
                        return;
                    }
                    Ok(Begin::Ordinary) => {}
                    Err(why) => {
                        let _ = reply.send(wire::Reply::err(why));
                        return;
                    }
                }
                let before = self.session.focus;
                let answer = self.ask(&req);
                // A caller outside the session has no screen of its own, so
                // "focus this" means every screen. Moving one client's and not
                // the other's would make which one an accident of ordering.
                if self.session.focus != before {
                    let now = self.session.focus;
                    for view in &mut self.views {
                        view.focus = now;
                    }
                }
                let _ = reply.send(answer);
            }
            Ev::Watch { watcher, back } => {
                let answer = self.begin_watch(*watcher);
                let _ = back.send(answer);
            }
            Ev::Watched(id, event) => self.watched(id, event),
            Ev::Unwatch(id) => {
                self.watchers.retain(|w| w.id != id);
            }
            Ev::Detach(id) => {
                // By id, so one client leaving cannot take another's view with
                // it. The session does not end because nobody is watching.
                self.views.retain(|v| v.id != id);
                // Panes that exist keep the size they had -- what is in them
                // was drawn for it. But the next one made belongs to a session
                // nobody is watching, and sizing it from the terminal that
                // just left is sizing it from a screen that no longer exists.
                if self.views.is_empty() {
                    self.content = self.headless;
                    self.shared = None;
                }
                self.resize_panes();
            }
            Ev::Output(id) => {
                self.session.touch(id);
                self.session.track_intents(&self.cfg.naming);
                self.session.name_tabs();
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
                self.session.name_tabs();
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
            // notification; what it does not do is make a noise. The harness
            // is asked first and the project second, because the more specific
            // answer is the one somebody wrote about this exact thing: three
            // agents in one repository is the case that needed this, and a
            // project answer cannot tell them apart.
            let agent = match change.at {
                Focus::Ws { p, w } => self
                    .session
                    .workspace(p, w)
                    .and_then(|ws| ws.active_pane())
                    .and_then(|pane| pane.occupant.agent())
                    .map(|k| k.name.clone()),
                Focus::Layout(_) => None,
            };
            let quiet = match self.cfg.sound.about(agent.as_deref()) {
                config::Says::Yes => false,
                config::Says::No => true,
                config::Says::Nothing => self.project_of(change.at).is_some_and(|p| !p.sound),
            };
            self.alert(
                change.to,
                format!("{} {what}", change.label),
                change.label.clone(),
                None,
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
    fn alert(
        &mut self,
        state: agent::State,
        title: String,
        label: String,
        said: Option<String>,
        quiet: bool,
    ) {
        let which = match state {
            agent::State::Blocked => sound::Alert::Blocked,
            agent::State::Done => sound::Alert::Done,
            _ => return,
        };
        // Every client watching, because every one of them is a person who
        // asked to be told. The floor and the seen rule already decided that
        // this is worth saying; who is looking is not that decision.
        if !self.views.is_empty() {
            let msg = wire::Alert {
                state: match quiet {
                    // Said rather than dropped: the notification is still owed,
                    // and only the noise was refused.
                    true => format!("{}-quiet", which.name()),
                    false => which.name().to_string(),
                },
                label,
                text: said.unwrap_or_default(),
            };
            for view in &mut self.views {
                let _ = wire::send_json(&mut view.out, wire::Kind::Alert, &msg);
            }
            return;
        }
        notify::send(title, self.cfg.identity.app());
        if !quiet {
            sound::play(&self.cfg.sound, which);
        }
    }

    /// Accept a direct attach, or say why not.
    ///
    /// The pane is resolved here rather than by the socket thread because only
    /// this loop knows what exists, and it is the same reason every other
    /// command is answered here.
    fn begin_watch(&mut self, mut watcher: server::Watcher) -> bool {
        let refuse = |watcher: &mut server::Watcher, why: String| {
            let _ = wire::send_json(&mut watcher.out, wire::Kind::Reply, &wire::Reply::err(why));
            false
        };
        let Some(pane) = api::target_pane(&self.session, &watcher.target) else {
            let why = format!("no such pane: {}", watcher.target);
            return refuse(&mut watcher, why);
        };
        // One writer at a time. Two terminals typing into one shell is not a
        // feature anybody asked for, and the character interleaving would be
        // blamed on the program rather than on this.
        if let Some(held) = self.watchers.iter().position(|w| w.pane == pane) {
            if !watcher.takeover {
                return refuse(
                    &mut watcher,
                    "another terminal is attached to that pane; pass --takeover to replace it"
                        .into(),
                );
            }
            // Dropped rather than told: the usual reason for a takeover is that
            // the other end is a terminal somebody has already closed.
            self.watchers.remove(held);
        }
        watcher.pane = pane;
        let said = wire::Reply::ok(serde_json::json!({
            "pane": api::pane_id_of(&self.session, pane),
            "rows": watcher.rows,
            "cols": watcher.cols,
        }));
        if wire::send_json(&mut watcher.out, wire::Kind::Reply, &said).is_err() {
            return false;
        }
        // The attached terminal owns the size, which is what makes this a
        // terminal for that pane rather than a window onto somebody else's.
        self.session.resize_pane(pane, watcher.rows, watcher.cols);
        self.watchers.push(watcher);
        // The current screen follows the acceptance rather than waiting for the
        // pane to say something next: attaching to a quiet pane should show you
        // what is in it, not an empty terminal.
        self.repost();
        true
    }

    /// A key, a paste or a wheel from a terminal attached to one pane.
    fn watched(&mut self, id: u64, event: Event) {
        let Some(pane) = self.watchers.iter().find(|w| w.id == id).map(|w| w.pane) else {
            return;
        };
        match event {
            // Plain PageUp and PageDown move through what has gone past, and
            // with a modifier they go to the program. Taking them outright
            // would take them off `less` and off every agent's transcript,
            // which is a worse trade than not having them here.
            Event::Key(k)
                if k.modifiers.is_empty()
                    && matches!(k.code, KeyCode::PageUp | KeyCode::PageDown) =>
            {
                let back = self.session.scrolled_at(pane);
                let page = self
                    .watchers
                    .iter()
                    .find(|w| w.id == id)
                    .map_or(1, |w| usize::from(w.rows.saturating_sub(1)).max(1));
                let to = match k.code {
                    KeyCode::PageUp => back + page,
                    _ => back.saturating_sub(page),
                };
                self.session.scroll_to(pane, to);
            }
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                // Typing puts the view back at the bottom. Sending a keystroke
                // to a program whose output you cannot see is the kind of thing
                // you find out about afterwards.
                self.session.scroll_to(pane, 0);
                self.session.keys_to(pane, &[k]);
            }
            Event::Paste(text) => {
                self.session.scroll_to(pane, 0);
                self.session.write_to(pane, text.as_bytes());
            }
            Event::Mouse(m) => {
                let back = self.session.scrolled_at(pane);
                let step = 3;
                match m.kind {
                    MouseEventKind::ScrollUp => self.session.scroll_to(pane, back + step),
                    MouseEventKind::ScrollDown => {
                        self.session.scroll_to(pane, back.saturating_sub(step));
                    }
                    _ => {}
                }
            }
            Event::Resize(cols, rows) => {
                if let Some(w) = self.watchers.iter_mut().find(|w| w.id == id) {
                    w.cols = cols;
                    w.rows = rows;
                    // Repainted from scratch at the new size: what was posted
                    // for the old one is wrong everywhere.
                    w.last.clear();
                }
                self.session.resize_pane(pane, rows, cols);
            }
            _ => {}
        }
    }

    /// Everything that happens after a batch of events, whichever loop drained
    /// them.
    ///
    /// One place, because there are two loops — the session's and the one that
    /// runs with no session at all — and they had already drifted: the second
    /// was not doing any of the work the first had gained since.
    fn after_events(&mut self) {
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

        // Everywhere anybody is looking. With nobody attached that is where
        // the session itself is pointed, which is what it was before there
        // could be more than one client.
        let watched: Vec<Focus> = match self.views.is_empty() {
            true => vec![self.session.focus],
            false => self.views.iter().map(|v| v.focus).collect(),
        };
        let changes = self.session.update_states(Instant::now(), &watched);
        self.announce(changes);
        // After the states, not before: a wait for `blocked` should end on the
        // turn the agent became blocked rather than on the next tick after it.
        self.settle();
        self.repost();
        self.retitle();
    }

    /// Say what the terminal dirk is running in should be called.
    ///
    /// Composed here because these are the session's facts — `{host}` is the
    /// machine the panes are on, which under `--remote` is not the machine the
    /// window is on — and written by whoever has a terminal.
    fn retitle(&mut self) {
        let ws = self.session.focused_workspace();
        let workspace = ws.map(|w| w.label.clone()).unwrap_or_default();
        let tab = ws
            .filter(|w| w.tabs.len() > 1)
            .map(|w| w.tab_label(w.tab))
            .unwrap_or_default();
        let pane = self
            .session
            .active_pane()
            .and_then(|p| p.title())
            .unwrap_or_default();
        let want = self.cfg.ui.title(&hostname(), &workspace, &tab, &pane);
        let Some(want) = want else { return };
        if self.titled.as_ref() == Some(&want) {
            return;
        }
        self.titled = Some(want.clone());
        if self.views.is_empty() {
            // No client: this is the single-process mode, and the terminal is
            // ours to write to directly.
            if self.socket.is_none() {
                let _ = execute!(io::stdout(), crossterm::terminal::SetTitle(&want));
            }
            return;
        }
        for view in &mut self.views {
            let _ = wire::send(&mut view.out, wire::Kind::Title, want.as_bytes());
        }
    }

    /// Post each attached pane's screen to whoever is watching it.
    ///
    /// Whole screens rather than the bytes the pane produced. dirk already has
    /// exactly one renderer for exactly this reason — two of them drift, the
    /// local one gets a fix and the remote one does not — and a screen that has
    /// not changed is not sent at all, so a still terminal stays still.
    fn repost(&mut self) {
        if self.watchers.is_empty() {
            return;
        }
        let mut gone = Vec::new();
        for i in 0..self.watchers.len() {
            let pane = self.watchers[i].pane;
            let Some(screen) = self.session.pane_screen(pane) else {
                gone.push(self.watchers[i].id);
                continue;
            };
            if screen == self.watchers[i].last {
                continue;
            }
            self.watchers[i].last = screen.clone();
            let out = &mut self.watchers[i].out;
            if wire::send(out, wire::Kind::Frame, &screen).is_err() {
                gone.push(self.watchers[i].id);
            }
        }
        self.watchers.retain(|w| !gone.contains(&w.id));
    }

    /// What happens to a command that the ordinary answer path does not handle.
    ///
    /// `Ordinary` falls through to `ask`. `Err` is a command that can never be
    /// answered — a target that does not exist, a state that does not — and
    /// those are refused at the door rather than waited out, because a caller
    /// that misspelled a state should not learn about it when its timeout
    /// expires.
    fn begin(&mut self, req: &wire::Request) -> Result<Begin, String> {
        let (words, opts) = wait::options(&req.args);
        // A read of more lines than the screen holds, from a pane whose program
        // keeps its history to itself. Everything else about `pane.read` is
        // unchanged and answered where it always was -- this is narrow on
        // purpose, because it is the only read that moves anything.
        if req.cmd == "pane.read"
            && let Some(target) = words.first()
            && let Some(pane) = api::target_pane(&self.session, target)
        {
            let want: u16 = words
                .get(1)
                .and_then(|n| n.parse().ok())
                .unwrap_or(api::READ_LINES);
            let rows = self
                .session
                .pane_visible(pane)
                .map_or(0, |s| s.lines().count());
            if usize::from(want) > rows && self.session.keeps_its_own_history(pane) {
                // Only an agent, and only one that has stopped. A screen
                // redrawing under this would be stitched out of two different
                // moments, and a caller would never know which.
                let at = api::locate(&self.session, pane);
                let state = at
                    .and_then(|(p, w)| self.session.workspace(p, w))
                    .map(|ws| ws.state);
                let known = at
                    .and_then(|(p, w)| api::agent_json(&self.session, p, w))
                    .is_some();
                if !known {
                    return Err(
                        "that pane keeps its own history and holds no agent dirk recognises; \
                         read it with --lines inside the screen"
                            .into(),
                    );
                }
                if !matches!(
                    state,
                    Some(agent::State::Idle) | Some(agent::State::Done) | Some(agent::State::None)
                ) {
                    return Err(
                        "that agent is not idle; its screen would be read from two moments".into(),
                    );
                }
                if self.session.scrolled_at(pane) != 0 {
                    return Err(
                        "that pane is being read from the past; nothing will move its viewport"
                            .into(),
                    );
                }
                return Ok(Begin::Waiting(wait::What::Transcript {
                    pane,
                    want,
                    pages: Vec::new(),
                    sent: 0,
                    ready: None,
                }));
            }
        }
        if req.cmd == "pane.wait-output" {
            if let Some(bad) = wait::unknown(&opts, &["regex", "lines", "timeout"]) {
                return Err(format!("pane.wait-output takes no --{bad}"));
            }
            let Some(target) = words.first() else {
                return Err("pane.wait-output needs a pane".into());
            };
            let Some(pane) = api::target_pane(&self.session, target) else {
                return Err(format!("no such pane: {target}"));
            };
            let text = words[1..].join(" ");
            if text.is_empty() {
                return Err("pane.wait-output needs something to look for".into());
            }
            return Ok(Begin::Waiting(wait::What::Output {
                pane,
                looking_for: wait::Match::read(&text, &opts)?,
                lines: wait::lines(&opts, api::READ_LINES)?,
            }));
        }
        if req.cmd == "agent.prompt" {
            if let Some(bad) = wait::unknown(&opts, &["wait", "until", "timeout"]) {
                return Err(format!("agent.prompt takes no --{bad}"));
            }
            let Some(target) = words.first() else {
                return Err("agent.prompt needs a workspace or a pane".into());
            };
            let Some((p, w)) = api::target_workspace(&self.session, target) else {
                return Err(format!("no such workspace or pane: {target}"));
            };
            if api::agent_json(&self.session, p, w).is_none() {
                return Err(format!("no agent in {target}"));
            }
            let text = words[1..].join(" ");
            if text.is_empty() {
                return Err("agent.prompt needs something to say".into());
            }
            let state = self.session.workspace(p, w).map(|ws| ws.state);
            // An agent sitting on a question is not one to type a prompt at.
            // The dialog wants an answer, and a prompt would be read as one --
            // so the caller is sent to look at it rather than having its input
            // silently become a menu selection.
            if state == Some(agent::State::Blocked) {
                return Err(
                    "that agent is blocked; read it and answer with agent send-keys".into(),
                );
            }
            let Some(pane) = self
                .session
                .workspace(p, w)
                .and_then(|ws| ws.active_pane())
                .map(|pane| pane.id)
            else {
                return Err(format!("no agent in {target}"));
            };
            if !self.session.submit(pane, &text) {
                return Err("that pane's program has exited".into());
            }
            // Answered here rather than by `ask`, because the writing has
            // already happened: a second pass over the same command would
            // either submit it twice or have to remember that it must not.
            let said = api::agent_json(&self.session, p, w).unwrap_or(serde_json::Value::Null);
            if !opts.iter().any(|(k, _)| k == "wait") {
                return Ok(Begin::Answered(serde_json::json!({ "agent": said })));
            }
            let until = wait::until(&opts)?;
            return Ok(Begin::Waiting(wait::What::Prompt {
                pane,
                until,
                started_by: Some(Instant::now() + wait::STARTS_WITHIN),
            }));
        }
        if req.cmd != "agent.wait" {
            return Ok(Begin::Ordinary);
        }
        if let Some(bad) = wait::unknown(&opts, &["until", "timeout"]) {
            return Err(format!("agent.wait takes no --{bad}"));
        }
        let Some(target) = words.first() else {
            return Err("agent.wait needs a workspace or a pane".into());
        };
        let Some((p, w)) = api::target_workspace(&self.session, target) else {
            return Err(format!("no such workspace or pane: {target}"));
        };
        if api::agent_json(&self.session, p, w).is_none() {
            return Err(format!("no agent in {target}"));
        }
        let Some(pane) = self
            .session
            .workspace(p, w)
            .and_then(|ws| ws.active_pane())
            .map(|pane| pane.id)
        else {
            return Err(format!("no agent in {target}"));
        };
        let until = wait::until(&opts)?;
        Ok(Begin::Waiting(wait::What::Agent { pane, until }))
    }

    /// Answer every held question that can now be answered.
    ///
    /// Taken and put back rather than iterated in place: deciding one needs the
    /// session, and holding a borrow of `self.waits` across that would make the
    /// check and the answer two different moments.
    fn settle(&mut self) {
        if self.waits.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut waiting = Vec::new();
        for mut held in std::mem::take(&mut self.waits) {
            match self.verdict(&mut held.what, held.deadline, now) {
                Some(settled) => {
                    let _ = held.back.send(settled.reply());
                }
                None => waiting.push(held),
            }
        }
        self.waits = waiting;
    }

    /// Has this happened, become impossible, or run out of time?
    fn verdict(
        &mut self,
        what: &mut wait::What,
        deadline: Option<Instant>,
        now: Instant,
    ) -> Option<wait::Settled> {
        let reached = match what {
            wait::What::Agent { pane, until } => {
                // By pane, so the wait survives the workspace being renamed,
                // refocused or reordered under it -- all of which happen to a
                // workspace an agent is working in, continuously.
                let Some((p, w)) = api::locate(&self.session, *pane) else {
                    return Some(wait::Settled::Gone("that pane is gone"));
                };
                let Some(agent) = api::agent_json(&self.session, p, w) else {
                    return Some(wait::Settled::Gone("that agent is no longer running"));
                };
                let state = self.session.workspace(p, w).map(|ws| ws.state);
                state
                    .is_some_and(|s| until.contains(&s))
                    .then_some(serde_json::json!({ "agent": agent }))
            }
            // The one held question that acts. It presses the same key
            // somebody with a wheel would, waits for the program to redraw,
            // and does it again -- so the whole of it lives on the loop that
            // would otherwise be the thing processing that redraw.
            wait::What::Transcript {
                pane,
                want,
                pages,
                sent,
                ready,
            } => {
                let Some(screen) = self.session.pane_visible(*pane) else {
                    return Some(wait::Settled::Gone("that pane is gone"));
                };
                match ready {
                    // Nothing asked for yet: take what is on the screen, and
                    // ask for the page above it.
                    None => {
                        pages.push(screen);
                        self.session.wheel(*pane, true, wait::WHEEL);
                        *sent += wait::WHEEL;
                        *ready = Some(now + wait::REDRAW);
                        return None;
                    }
                    Some(at) if now < *at => return None,
                    Some(_) => {}
                }

                // A page identical to the one before it means the program did
                // not move: either there is no more history, or it is not
                // listening. Either way this is the end, and stopping here is
                // what keeps `stitch` from being handed a duplicate.
                let ended = pages.last().is_some_and(|last| *last == screen);
                if !ended {
                    pages.push(screen);
                }
                let have = wait::stitch(pages).lines().count();
                if ended || have >= usize::from(*want) || pages.len() >= wait::PAGES {
                    // Put it back before answering, always. A caller's read
                    // must not leave somebody's agent scrolled into its own
                    // past.
                    self.session.wheel(*pane, false, *sent);
                    let text = wait::stitch(pages);
                    let text = match text.lines().count() > usize::from(*want) {
                        // The oldest lines are the ones nobody asked for: a
                        // read of forty lines means the last forty.
                        true => text
                            .lines()
                            .skip(text.lines().count() - usize::from(*want))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        false => text,
                    };
                    return Some(wait::Settled::Reached(serde_json::json!({
                        "text": text,
                        "pages": pages.len(),
                    })));
                }
                self.session.wheel(*pane, true, wait::WHEEL);
                *sent += wait::WHEEL;
                *ready = Some(now + wait::REDRAW);
                return None;
            }

            wait::What::Prompt {
                pane,
                until,
                started_by,
            } => {
                let Some((p, w)) = api::locate(&self.session, *pane) else {
                    return Some(wait::Settled::Gone("that pane is gone"));
                };
                let Some(agent) = api::agent_json(&self.session, p, w) else {
                    return Some(wait::Settled::Gone("that agent is no longer running"));
                };
                let Some(state) = self.session.workspace(p, w).map(|ws| ws.state) else {
                    return Some(wait::Settled::Gone("that agent is no longer running"));
                };
                if let Some(by) = *started_by {
                    let working = matches!(state, agent::State::Working | agent::State::Blocked);
                    if working {
                        *started_by = None;
                    } else if now >= by {
                        return Some(wait::Settled::Gone(
                            "the prompt was sent and nothing started; read the agent \
                             before sending it again",
                        ));
                    } else {
                        // Still might. An `idle` here is the state the agent
                        // was in before the prompt, not an answer to it.
                        return None;
                    }
                }
                until
                    .contains(&state)
                    .then_some(serde_json::json!({ "agent": agent }))
            }
            wait::What::Output {
                pane,
                looking_for,
                lines,
            } => {
                let Some(text) = self.session.pane_text(*pane, *lines) else {
                    return Some(wait::Settled::Gone("that pane is gone"));
                };
                // Searched the moment it is asked as well as on every turn
                // after, so a caller that starts a command and then waits for
                // its output does not lose the race to text already on screen.
                looking_for.first_in(&text).map(|line| {
                    serde_json::json!({
                        "pane": api::pane_id_of(&self.session, *pane),
                        "matched": line,
                    })
                })
            }
        };
        match reached {
            Some(value) => Some(wait::Settled::Reached(value)),
            // Checked after, so a wait whose condition and whose deadline land
            // on the same turn is answered rather than timed out.
            None => deadline
                .is_some_and(|at| now >= at)
                .then_some(wait::Settled::Late(what.on_timeout())),
        }
    }

    /// Do what was asked, or say why not.
    ///
    /// Reads are answered by `api::read` and never touch the seen rule: asking
    /// about a workspace is not looking at one, and without that a status line
    /// polling the session would clear every notification it exists to show.
    fn ask(&mut self, req: &wire::Request) -> wire::Reply {
        use wire::Reply;
        if let Some(answer) = api::read(&self.session, &self.kinds, &req.cmd, &req.args) {
            return answer;
        }
        let area = self.content;
        let arg = |n: usize| req.args.get(n).cloned().unwrap_or_default();

        match req.cmd.as_str() {
            "session.reload" => match Config::reload(&mut self.cfg, &mut self.session) {
                Ok(said) => {
                    // Rebuilt here, which is the only place it can change. The
                    // rule files are read again too: a reload that took the
                    // config and not the rules beside it would be a reload
                    // somebody had to know the shape of.
                    self.glyphs = self.cfg.nav.glyphs();
                    let (kinds, mut notes) = self.cfg.kinds_and_complaints();
                    self.kinds = std::sync::Arc::new(kinds);
                    let mut said = said;
                    said.append(&mut notes);
                    Reply::ok(serde_json::json!({
                        "reloaded": true,
                        // Whatever the file was wrong about. Empty is the usual
                        // answer and the only one worth not reading.
                        "notes": said,
                    }))
                }
                Err(e) => Reply::err(e),
            },

            // The counterpart to attaching and pressing `q`. A session you
            // want gone should not require a terminal to go and stand in.
            // The same path an agent's blocking takes, and therefore the same
            // rules: nothing about the workspace somebody is looking at, not
            // more often than the floor, and no noise for a project that asked
            // to be quiet. A build, a deploy or a cron job is owed exactly what
            // an agent is owed and no more.
            "session.notify" => {
                let (words, opts) = wait::options(&req.args);
                if let Some(bad) = wait::unknown(&opts, &["blocked"]) {
                    return Reply::err(format!("session.notify takes no --{bad}"));
                }
                let Some(target) = words.first() else {
                    return Reply::err("session.notify needs a workspace or a pane");
                };
                let Some(at) = api::target_workspace(&self.session, target) else {
                    return Reply::err(format!("no such workspace or pane: {target}"));
                };
                let said = words[1..].join(" ");
                if said.is_empty() {
                    return Reply::err("session.notify needs something to say");
                }
                // Two events are worth hearing and they have to be told apart
                // with your back to the screen. A script that finished is the
                // ordinary one; a script that needs you is the interruption.
                let state = match opts.iter().any(|(k, _)| k == "blocked") {
                    true => agent::State::Blocked,
                    false => agent::State::Done,
                };
                let (p, w) = at;
                let focus = Focus::Ws { p, w };
                let watched = match self.views.is_empty() {
                    true => self.session.focus == focus,
                    false => self.views.iter().any(|v| v.focus == focus),
                };
                let label = self
                    .session
                    .workspace(p, w)
                    .map(|ws| ws.label.clone())
                    .unwrap_or_default();
                let why = if !self.cfg.notify.enabled {
                    Some("notifications are off")
                } else if watched {
                    Some("you are looking at it")
                } else if !self.session.may_notify(
                    focus,
                    state,
                    Instant::now(),
                    Duration::from_millis(self.cfg.notify.min_interval_ms),
                ) {
                    Some("too soon after the last one")
                } else {
                    None
                };
                match why {
                    Some(why) => Reply::ok(serde_json::json!({
                        "notified": false, "why": why, "workspace": target,
                    })),
                    None => {
                        let quiet = self.project_of(focus).is_some_and(|d| !d.sound);
                        self.alert(state, said.clone(), label, Some(said), quiet);
                        Reply::ok(serde_json::json!({
                            "notified": true, "workspace": target,
                        }))
                    }
                }
            }

            "session.quit" => {
                self.quit = true;
                Reply::ok(serde_json::json!({ "quit": true }))
            }

            "session.info" => Reply::ok(serde_json::json!({
                "workspaces": self.session.flat().len(),
                "layouts": self.session.layouts.len(),
                "attached": !self.views.is_empty(),
                "clients": self.views.len(),
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
                let made = self
                    .session
                    .new_workspace_at(p, &path, area.height, area.width);
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
                        .map(|ws| ws.panes().iter().map(|x| x.id).collect())
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
                            ws.set_focus(id);
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
                    ws.set_focus(id);
                }
                self.session.split(dir, area.height, area.width);
                let new = self.session.workspace(p, w).map(|ws| ws.focus());
                self.session.focus = was;

                match new {
                    Some(pane) => {
                        let ws_id = self.session.workspace(p, w).map(|x| x.id).unwrap_or(0);
                        Reply::ok(serde_json::json!({ "pane": api::pane_id(ws_id, pane) }))
                    }
                    None => Reply::err("could not split"),
                }
            }

            // Three verbs where there was one. A command, literal text and a
            // keystroke fail in different ways -- text can be pasted, a key
            // cannot, and a command needs both in an order that is guaranteed
            // -- so a single verb meant every caller wrote the ordering itself
            // and got it wrong against anything slow to read.
            "pane.run" | "pane.send-text" => {
                let Some(id) = api::target_pane(&self.session, &arg(0)) else {
                    return Reply::err("no such pane");
                };
                if self.session.pane_alive(id) == Some(false) {
                    return Reply::err("that pane's program has exited");
                }
                let text = req.args[1..].join(" ");
                if text.is_empty() {
                    return Reply::err(format!("{} needs something to send", req.cmd));
                }
                // The submitting return goes in the same write as the command.
                // Two writes is two chances for a program reading slowly to
                // see a bare newline and run whatever it had, which is how a
                // caller ends up having typed half a command.
                let mut bytes = text.clone().into_bytes();
                if req.cmd == "pane.run" {
                    bytes.push(b'\r');
                }
                match self.session.write_to(id, &bytes) {
                    true => Reply::ok(serde_json::json!({ "sent": text })),
                    false => Reply::err("no such pane"),
                }
            }

            "pane.send-keys" => {
                let Some(id) = api::target_pane(&self.session, &arg(0)) else {
                    return Reply::err("no such pane");
                };
                if self.session.pane_alive(id) == Some(false) {
                    return Reply::err("that pane's program has exited");
                }
                if req.args.len() < 2 {
                    return Reply::err("pane.send-keys needs a key");
                }
                let mut keys = Vec::new();
                for name in &req.args[1..] {
                    match crate::keys::named(name) {
                        Some(k) => keys.push(k),
                        None => return Reply::err(format!("no such key: {name}")),
                    }
                }
                match self.session.keys_to(id, &keys) {
                    true => Reply::ok(serde_json::json!({ "sent": req.args[1..] })),
                    false => Reply::err("that key has no sequence on this terminal"),
                }
            }

            // Display, deliberately not state. `agent state` is a small closed
            // set dirk reasons about — it drives waits, notifications, ordering
            // and the attention column — and it has to stay that way. This is
            // where everything a program wants to *show* goes instead, so an
            // indexer's progress stops having to become a state in order to be
            // visible.
            "pane.metadata" => {
                let Some(id) = api::target_pane(&self.session, &arg(0)) else {
                    return Reply::err("no such pane");
                };
                for pair in &req.args[1..] {
                    let Some((key, value)) = pair.split_once('=') else {
                        return Reply::err(format!("{pair:?} is not key=value"));
                    };
                    if key.is_empty() {
                        return Reply::err("a token needs a name");
                    }
                    self.session.report_metadata(id, key, value);
                }
                match self.session.metadata(id) {
                    Some(said) => Reply::ok(serde_json::json!({
                        "pane": arg(0),
                        "said": said,
                    })),
                    None => Reply::err("no such pane"),
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

            "tab.new" => {
                let (rows, cols) = (area.height, area.width);
                if let Some(at) = api::target_workspace(&self.session, &arg(0)) {
                    self.session.focus = Focus::Ws { p: at.0, w: at.1 };
                }
                match self.session.new_tab(rows, cols) {
                    Some(()) => {
                        let ws = self.session.focused_workspace();
                        let id = ws.map(|w| api::tab_id(w.id, w.here().id));
                        Reply::ok(serde_json::json!({ "tab": id }))
                    }
                    None => Reply::err("no space focused"),
                }
            }

            "tab.focus" | "tab.rename" | "tab.close" => {
                let Some(want) = api::parse_tab(&arg(0)) else {
                    return Reply::err("not a tab id");
                };
                let Some((p, w, t)) = self.find_tab(want) else {
                    return Reply::err("no such tab");
                };
                let verb = req.cmd.as_str();
                let Some(ws) = self.session.workspace_mut(p, w) else {
                    return Reply::err("no such tab");
                };
                match verb {
                    "tab.focus" => {
                        ws.tab = t;
                        self.session.focus = Focus::Ws { p, w };
                        Reply::ok(serde_json::json!({ "focused": arg(0) }))
                    }
                    "tab.rename" => {
                        let name = req.args[1..].join(" ");
                        ws.tabs[t].label = name.clone();
                        Reply::ok(serde_json::json!({ "renamed": name }))
                    }
                    _ => {
                        if ws.tabs.len() < 2 {
                            return Reply::err("the last tab is the space");
                        }
                        for pane in &mut ws.tabs[t].panes {
                            pane.close();
                        }
                        Reply::ok(serde_json::json!({ "closed": arg(0) }))
                    }
                }
            }

            "worktree.list" => {
                let Some(dir) = self.here() else {
                    return Reply::err("no project focused");
                };
                let here = self.session.focused_dir();
                Reply::ok(serde_json::json!({
                    "worktrees": git::worktrees(&dir)
                        .into_iter()
                        .map(|t| serde_json::json!({
                            "path": t.path,
                            "branch": t.branch,
                            "main": t.main,
                            // Which one you are in, since the answer is a list
                            // of directories that all look alike.
                            "open": here.as_deref() == Some(t.path.as_path()),
                        }))
                        .collect::<Vec<_>>(),
                }))
            }

            "worktree.add" => {
                let branch = arg(0);
                if branch.is_empty() {
                    return Reply::err("which branch?");
                }
                match self.add_worktree(&branch) {
                    Ok(at) => Reply::ok(serde_json::json!({
                        "branch": branch,
                        "path": at,
                    })),
                    Err(why) => Reply::err(why),
                }
            }

            "worktree.remove" => {
                let Some(dir) = self.here() else {
                    return Reply::err("no project focused");
                };
                let want = arg(0);
                let force = req.args.iter().any(|a| a == "--force");
                // By branch or by path, because a branch is what you remember
                // and a path is what `worktree list` gave you.
                let Some(tree) = git::worktrees(&dir)
                    .into_iter()
                    .find(|t| t.branch == want || t.path.to_string_lossy() == want)
                else {
                    return Reply::err(format!("no worktree for {want}"));
                };
                if tree.main {
                    return Reply::err("that is the repository, not a worktree of it");
                }
                match git::remove(&dir, &tree.path, force) {
                    Ok(()) => {
                        self.session.close_checkout(&tree.path);
                        Reply::ok(serde_json::json!({ "removed": tree.path }))
                    }
                    Err(why) => Reply::err(why),
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
                // The harness's own name for this conversation, when the hook
                // passed one along. Taken before the workspace is borrowed
                // mutably, because reading it needs the kinds.
                let named = req
                    .args
                    .iter()
                    .position(|a| a == "--session")
                    .and_then(|i| req.args.get(i + 1))
                    .filter(|id| !id.is_empty())
                    .cloned();
                let kind = self
                    .session
                    .workspace(p, w)
                    .and_then(|ws| ws.active_pane())
                    .and_then(|pane| pane.occupant.agent())
                    .map(|k| k.name.clone());
                let Some(ws) = self.session.workspace_mut(p, w) else {
                    return Reply::err("no such workspace");
                };
                ws.reported = Some((state, Instant::now()));
                // Kept only alongside the harness it came from: a reference is
                // only meaningful to the program that issued it, and resuming
                // codex on claude's id would start a conversation nobody had.
                if let (Some(id), Some(kind)) = (named, kind) {
                    ws.agent_session = Some((kind, id));
                }
                Reply::ok(serde_json::json!({
                    "workspace": api::workspace_id(ws.id),
                    "state": state.glyph_name(),
                    "session": ws.agent_session.as_ref().map(|(_, id)| id.clone()),
                }))
            }

            "agent.start" => {
                let Some(kind) = self.kinds.iter().find(|k| k.name == arg(0)).cloned() else {
                    return Reply::err(format!("no such agent: {}", arg(0)));
                };
                if kind.command.is_empty() {
                    return Reply::err(format!("{} has no command to start it with", kind.name));
                }
                // No pane means the one you are in, the same way no workspace
                // means the one you are looking at. A target that was given and
                // did not resolve is still an error.
                let target = match arg(1).is_empty() {
                    true => self.session.focused_workspace().map(|ws| ws.focus()),
                    false => api::target_pane(&self.session, &arg(1)),
                };
                let Some(pane) = target else {
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
            .map(|ws| ws.focus())
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
        let wrappers = self.cfg.wrappers();
        std::thread::spawn(move || {
            let table = crate::agent::table();
            let mut out = Vec::new();
            let mut hinted = Vec::new();
            for (id, pgid) in panes {
                let proc = pgid.and_then(|pgid| table.get(&pgid));
                let occupant = match pgid {
                    // Nothing in the foreground: the pane is holding nothing,
                    // and that is news.
                    None => Some(crate::agent::Occupant::Unknown),
                    // A group that is not in the table ended between the pgid
                    // being read and `ps` running. That is not news.
                    Some(_) => proc.map(|p| crate::agent::identify(p, &kinds, &wrappers)),
                };
                // Recognised through something rather than as itself. Recorded
                // so that `agent explain` can say so, rather than leaving
                // somebody to wonder how dirk came to recognise `fence`.
                if let (Some(crate::agent::Occupant::Agent(kind)), Some(proc)) =
                    (occupant.as_ref(), proc)
                    && let Some(wrapper) = crate::agent::behind(proc, kind)
                {
                    hinted.push((id, wrapper));
                }
                out.push((id, occupant));
            }
            let _ = tx.send(Ev::Agents(crate::agent::Reading { panes: out, hinted }));
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
            for w in 0..self.session.projects[p].workspaces.len() {
                // Without this, a workspace labelled with its own branch name
                // looks hand-written and naming backs off from it permanently.
                // Per space rather than per project: with worktrees open, two
                // spaces in one project are on two branches.
                let branch = self.session.projects[p]
                    .repo_of(w)
                    .map(|r| r.branch.clone())
                    .unwrap_or_default();
                let ws = &self.session.projects[p].workspaces[w];
                // The title first, always. The second source is for the pane
                // that has none, and its candidate goes through this same
                // policy with no exemptions.
                let title = ws
                    .active_pane()
                    .and_then(|x| x.title())
                    .or_else(|| ws.suggested.clone());
                let panes = ws.panes().len();
                let here = name::where_it_is(ws);
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
                    &here,
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

    fn visible_pane(&self, index: usize) -> Option<&Pane> {
        let ws = self.session.focused_workspace()?;
        let id = *ws.tree().leaves().get(index)?;
        ws.pane(id)
    }

    fn visible_pane_mut(&mut self, index: usize) -> Option<&mut Pane> {
        match self.session.focus {
            Focus::Layout(i) => {
                let ws = self.session.layouts.get_mut(i)?.ws.as_mut()?;
                let id = *ws.tree().leaves().get(index)?;
                ws.pane_mut(id)
            }
            Focus::Ws { p, w } => {
                let ws = self.session.workspace_mut(p, w)?;
                let id = *ws.tree().leaves().get(index)?;
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
        // The panes are as wide as the narrowest screen showing them, which is
        // not necessarily this one. Only when nobody else is looking does this
        // view's own size decide.
        self.session.resize_visible(self.shared.unwrap_or(content));

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
        draw_content(
            buf,
            content,
            &self.glyphs,
            &self.session,
            self.copy.as_ref(),
            &mut self.hits,
        );

        let clock = chrono::Local::now().format("%H:%M").to_string();
        ui::rail::render(
            buf,
            rail,
            &self.cfg,
            &self.glyphs,
            &self.session,
            &ui::rail::Now {
                clock: &clock,
                note: &self.status,
                // Not folded into the note it used to be. A chord waiting for
                // its second key is the most time-critical thing the interface
                // says, and the rail draws it as such.
                prefix: self.prefix,
                // Being read from the past is a state you can forget you are
                // in, and it is the one surface on every screen.
                scrolled: self.session.scrolled(),
                quit_armed: self.quit_armed.is_some(),
                session: self.session_name.as_deref(),
                // The rail's middle shows what the nav is not showing, and a
                // sidebar of no width is a sidebar that is not showing it.
                nav_visible: side.width > 0,
            },
            &mut self.hits,
        );

        if let Some(f) = &self.find {
            ui::found::render(buf, content, &self.glyphs, f, &mut self.hits);
        }
        if let Some(a) = &self.asking {
            ui::ask::render(buf, content, &self.glyphs, a.what, &a.text);
        }
        if let Some(p) = &self.palette {
            ui::palette::render(buf, content, &self.glyphs, p, &mut self.hits);
        }
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
                mux::session::content_of(self.cfg.ui.ruled(pane.label.is_some()), rect),
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
                let at = ws.tree().leaves().iter().position(|&id| id == ws.focus())?;
                rects.get(at).copied()
            }
            Focus::Ws { p, w } => {
                let ws = self.session.workspace(p, w)?;
                let at = ws.tree().leaves().iter().position(|&id| id == ws.focus())?;
                rects.get(at).copied()
            }
        }
    }

    /// Do the thing the user pointed at, however they pointed at it.
    ///
    /// Clicking a row and pressing Enter on it arrive here with the same value,
    /// which is the only reason the two cannot drift apart.
    /// Whether a space has anything worth opening.
    ///
    /// Tabs as well as panes: a space with two tabs of one pane each has a
    /// subtree, and asking only about the tab on screen said it did not.
    fn holds_a_subtree(&self, p: usize, w: usize) -> bool {
        self.session
            .workspace(p, w)
            .is_some_and(|ws| ws.tabs.len() > 1 || ws.panes().len() > 1)
    }

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
                // A row you are already on cannot take you anywhere, so the
                // only thing left for that to mean is opening it. This is what
                // the keyboard does with the row it is sitting on; the pointer
                // has the mark at the right edge and does not need it.
                let here = self.session.focus == Focus::Ws { p, w };
                if here && self.holds_a_subtree(p, w) {
                    self.act(Target::SpaceFold { p, w });
                    return;
                }
                self.session.focus = Focus::Ws { p, w };
            }
            Target::SpaceFold { p, w } => {
                // The mark is only drawn on a row that has something under it,
                // but the keyboard reaches this by another road and a row can
                // lose its panes between frames.
                if !self.holds_a_subtree(p, w) {
                    return;
                }
                if let Some(ws) = self.session.workspace_mut(p, w) {
                    ws.expanded = !ws.expanded;
                }
            }
            Target::NavTab { p, w, t } => {
                self.session.focus = Focus::Ws { p, w };
                if let Some(ws) = self.session.workspace_mut(p, w)
                    && t < ws.tabs.len()
                {
                    ws.tab = t;
                }
            }
            Target::NavPane { p, w, t, index } => {
                self.session.focus = Focus::Ws { p, w };
                if let Some(ws) = self.session.workspace_mut(p, w)
                    && t < ws.tabs.len()
                {
                    ws.tab = t;
                    if let Some(&id) = ws.tabs[t].tree.leaves().get(index) {
                        ws.tabs[t].focus = id;
                    }
                }
            }
            Target::NewAgent => {
                self.new_agent();
                return;
            }
            Target::NewWorktree => {
                self.ask_for_worktree();
                return;
            }
            Target::NewWorkspace(p) => {
                self.session.new_workspace(p, area.height, area.width);
            }
            Target::OpenProject => {
                self.open_picker();
                return;
            }
            Target::PaletteRow(i) => {
                if let Some(p) = self.palette.as_mut()
                    && let Some(at) = p.matches().iter().position(|(j, _)| *j == i)
                {
                    p.selected = at;
                }
                self.palette_key(KeyEvent::from(KeyCode::Enter));
                return;
            }
            Target::FoundRow(i) => {
                if let Some(f) = self.find.as_mut() {
                    f.at = i;
                }
                self.show_found();
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
        if self.palette.is_some() && !self.prefix {
            return self.palette_key(k);
        }
        if self.asking.is_some() && !self.prefix {
            return self.ask_key(k);
        }
        if self.find.is_some() && !self.prefix {
            return self.find_key(k);
        }
        // Before the prefix: while selecting, keys move a cursor rather than
        // reaching the program, and that has to be escapable without one.
        if self.copy.is_some() && !self.prefix {
            return self.copy_key(k);
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

    fn open_palette(&mut self) {
        self.palette = Some(palette::Palette::new(&self.session, &self.keys));
    }

    /// Keys while the palette is open.
    fn palette_key(&mut self, k: KeyEvent) {
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        match k.code {
            KeyCode::Esc => self.palette = None,
            KeyCode::Down | KeyCode::Tab => p.move_by(1),
            KeyCode::Up | KeyCode::BackTab => p.move_by(-1),
            KeyCode::Backspace => {
                p.query.pop();
                p.reset();
            }
            KeyCode::Char(c) => {
                p.query.push(c);
                p.reset();
            }
            KeyCode::Enter => {
                let chosen = p
                    .matches()
                    .get(p.selected)
                    .map(|(i, _)| *i)
                    .and_then(|i| p.entry(i).map(|e| (e.what, e.why_not.clone())));
                let Some((what, why_not)) = chosen else {
                    return;
                };
                // Refused rather than done quietly wrong. The reason was on the
                // row; pressing anyway should say the same thing.
                if let Some(why) = why_not {
                    return self.note(&why);
                }
                self.palette = None;
                match what {
                    palette::What::Do(a) => self.perform(a),
                    palette::What::Go(focus) => {
                        // Through the same door a click uses, so a workspace
                        // that closed while the palette was open cannot leave
                        // focus pointing at nothing.
                        match focus {
                            Focus::Layout(i) => self.act(Target::Layout(i)),
                            Focus::Ws { p, w } => self.act(Target::Workspace { p, w }),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Ask which branch, then make a worktree for it.
    fn ask_for_worktree(&mut self) {
        if self.here().is_none() {
            return self.note("no project focused");
        }
        self.asking = Some(Ask {
            what: "worktree for branch",
            text: String::new(),
            then: Then::Worktree,
        });
    }

    /// Keys while a question is open.
    fn ask_key(&mut self, k: KeyEvent) {
        let Some(ask) = self.asking.as_mut() else {
            return;
        };
        match k.code {
            KeyCode::Esc => self.asking = None,
            KeyCode::Backspace => {
                ask.text.pop();
            }
            KeyCode::Char(c) => ask.text.push(c),
            KeyCode::Enter => {
                let Some(ask) = self.asking.take() else {
                    return;
                };
                let answer = ask.text.trim().to_string();
                if answer.is_empty() {
                    return;
                }
                match ask.then {
                    Then::Worktree => match self.add_worktree(&answer) {
                        Ok(at) => {
                            let name = at.file_name().unwrap_or_default().to_string_lossy();
                            self.note(&format!("opened {name}"));
                        }
                        Err(why) => self.note(&why),
                    },
                }
            }
            _ => {}
        }
    }

    /// Where a tab is, by its handle.
    fn find_tab(&self, id: u64) -> Option<(usize, usize, usize)> {
        self.session.flat().into_iter().find_map(|(p, w)| {
            let ws = self.session.workspace(p, w)?;
            ws.tabs.iter().position(|t| t.id == id).map(|t| (p, w, t))
        })
    }

    /// The directory of whatever is focused, which is what git is asked about.
    fn here(&self) -> Option<std::path::PathBuf> {
        self.session.focused_dir()
    }

    /// A worktree for `branch`, and a space open in it.
    ///
    /// One action because it is one intention. Doing it by hand is: leave
    /// dirk, `git worktree add`, come back, open the project -- and the reason
    /// parallel agents on one repository are practical at all is that this is
    /// the thing you do to start each of them.
    fn add_worktree(&mut self, branch: &str) -> Result<std::path::PathBuf, String> {
        let Some(dir) = self.here() else {
            return Err("no project focused".into());
        };
        // Beside the repository proper rather than beside whichever worktree
        // you happen to be standing in, or they nest.
        let main = git::worktrees(&dir)
            .into_iter()
            .find(|t| t.main)
            .map_or(dir.clone(), |t| t.path);
        let at = git::beside(&main, branch);
        if at.exists() {
            return Err(format!("{} is already there", at.display()));
        }
        let at = git::add(&dir, branch, &at)?;
        // Opened rather than only created: a worktree with nothing in it is a
        // directory, and what was wanted is somewhere to work.
        self.open(&at);
        Ok(at)
    }

    /// Exchange the focused pane with its neighbour.
    fn move_pane(&mut self, delta: isize) {
        let moved = self
            .session
            .focused_workspace_mut()
            .is_some_and(|ws| ws.move_focused(delta));
        if !moved {
            self.note("nothing to move it past");
        }
    }

    /// Read everything every pane has said, and start filtering it.
    fn start_find(&mut self) {
        let Some(ws) = self.session.focused_workspace() else {
            return self.note("nothing to search");
        };
        let origin = (ws.focus(), self.session.scrolled());
        self.find = Some(find::Find::new(self.session.everything_said(), origin));
        self.copy = None;
    }

    /// Keys while a search is open.
    fn find_key(&mut self, k: KeyEvent) {
        let Some(f) = self.find.as_mut() else { return };
        match k.code {
            KeyCode::Esc => self.leave_find(true),
            KeyCode::Enter => self.leave_find(false),
            // Down and up rather than n and N: this is a box you are typing
            // into, and a letter that moved the selection would be a letter you
            // could not search for.
            KeyCode::Down | KeyCode::Tab => {
                f.step(1);
                self.show_found();
            }
            KeyCode::Up | KeyCode::BackTab => {
                f.step(-1);
                self.show_found();
            }
            KeyCode::Backspace => {
                f.query.pop();
                f.refresh();
                self.show_found();
            }
            KeyCode::Char(c) => {
                f.query.push(c);
                f.refresh();
                self.show_found();
            }
            _ => {}
        }
    }

    /// Take the view to the match currently pointed at.
    ///
    /// The match is marked with the same span a selection uses, so it is
    /// highlighted by the code that already knows how to do that -- and it is
    /// there to be copied the moment the search closes.
    fn show_found(&mut self) {
        let Some((hit, line)) = self.find.as_ref().and_then(|f| f.current()) else {
            self.copy = None;
            return;
        };
        let (pane, back, row, col, len) = (line.pane, line.back, line.row, hit.col, hit.len);
        if let Some((p, w)) = api::locate(&self.session, pane) {
            self.session.focus = Focus::Ws { p, w };
        }
        if let Some(ws) = self.session.focused_workspace_mut() {
            ws.set_focus(pane);
        }
        self.session.scroll_to(pane, back);

        let mut mode = copy::Mode::new(pane, (row, col + len.saturating_sub(1)));
        mode.anchor = Some((row, col));
        self.copy = Some(mode);
    }

    /// Close the search. `back` puts you where you were, which is what leaving
    /// without choosing anything should cost.
    fn leave_find(&mut self, back: bool) {
        let Some(f) = self.find.take() else { return };
        self.copy = None;
        if back {
            let (pane, at) = f.origin;
            if let Some((p, w)) = api::locate(&self.session, pane) {
                self.session.focus = Focus::Ws { p, w };
            }
            if let Some(ws) = self.session.focused_workspace_mut() {
                ws.set_focus(pane);
            }
            self.session.scroll_to(pane, at);
        }
    }

    /// Start reading a pane rather than typing into it.
    ///
    /// The cursor starts where the program's is, because that is where you were
    /// looking. Nothing is written to the pty: the program does not learn that
    /// somebody is reading it.
    fn start_copy(&mut self) {
        let Some(ws) = self.session.focused_workspace() else {
            return self.note("nothing to read");
        };
        let pane = ws.focus();
        let at = ws
            .pane(pane)
            .and_then(|p| p.term.lock().ok().map(|t| t.screen().cursor_position()))
            .unwrap_or((0, 0));
        self.copy = Some(copy::Mode::new(pane, at));
        self.note("read: v select · y copy · esc");
    }

    /// Keys while a pane is being read.
    fn copy_key(&mut self, k: KeyEvent) {
        let Some(mode) = self.copy.as_mut() else {
            return;
        };
        let (rows, cols) = self
            .session
            .focused_workspace()
            .and_then(|ws| ws.pane(mode.pane))
            .and_then(|p| p.term.lock().ok().map(|t| t.screen().size()))
            .unwrap_or((24, 80));
        let step = |v: u16, d: isize, max: u16| {
            (v as isize + d).clamp(0, max.saturating_sub(1) as isize) as u16
        };
        // While a query is being typed, every key is part of it. A letter that
        // moved the cursor would be a letter you could not search for.
        if mode.search.as_ref().is_some_and(|s| s.typing) {
            return self.search_key(k);
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                // Escape clears a search before it leaves, so the first press
                // takes the highlight off and the second puts you back.
                if mode.search.take().is_some() && k.code == KeyCode::Esc {
                    self.note("read: v select · y copy · esc");
                    return;
                }
                self.copy = None;
                self.session.unscroll_focused();
                self.status.clear();
            }
            // `/` forward and `?` backward, `n` and `N` to repeat in the same
            // and the opposite direction: the pair every pager has.
            KeyCode::Char(c @ ('/' | '?')) => {
                mode.search = Some(copy::Search {
                    query: String::new(),
                    forward: c == '/',
                    typing: true,
                });
                self.note(&format!("{c}"));
            }
            KeyCode::Char(c @ ('n' | 'N')) => {
                let Some(search) = mode.search.clone() else {
                    return self.note("nothing to look for yet");
                };
                let forward = search.forward != (c == 'N');
                let pane = mode.pane;
                let from = mode.at;
                let lines = self.copy_lines(pane);
                match search.find(&lines, from, forward) {
                    Some(at) => {
                        if let Some(mode) = self.copy.as_mut() {
                            mode.at = at;
                        }
                    }
                    None => self.note(&format!("no {:?} on this screen", search.query)),
                }
            }
            // Anchoring where the cursor is, so `v` then a movement selects the
            // way it does in every editor that has this.
            KeyCode::Char('v') | KeyCode::Char(' ') => mode.anchor = Some(mode.at),
            KeyCode::Char('y') | KeyCode::Enter => self.take_selection(),
            KeyCode::Char('h') | KeyCode::Left => mode.at.1 = step(mode.at.1, -1, cols),
            KeyCode::Char('l') | KeyCode::Right => mode.at.1 = step(mode.at.1, 1, cols),
            KeyCode::Char('0') => mode.at.1 = 0,
            KeyCode::Char('$') => mode.at.1 = cols.saturating_sub(1),
            KeyCode::Char('k') | KeyCode::Up => match mode.at.0 {
                // At the top of the screen, up means further back rather than
                // nowhere -- which is the whole reason to be here.
                0 => {
                    self.session.scroll_focused(1);
                }
                _ => mode.at.0 = step(mode.at.0, -1, rows),
            },
            KeyCode::Char('j') | KeyCode::Down => match mode.at.0 + 1 >= rows {
                true => {
                    self.session.scroll_focused(-1);
                }
                false => mode.at.0 = step(mode.at.0, 1, rows),
            },
            // Half a screen, which is what the pair does everywhere else.
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.session.scroll_focused(rows as isize / 2);
            }
            KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.session.scroll_focused(-(rows as isize) / 2);
            }
            KeyCode::Char('b') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.session.scroll_focused(rows as isize);
            }
            KeyCode::Char('f') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.session.scroll_focused(-(rows as isize));
            }
            // vi's word motions, on the line the cursor is on. Holding `l` to
            // reach an identifier in a stack trace is the gap people actually
            // feel, because every editor they use has `w`.
            KeyCode::Char(c @ ('w' | 'b' | 'e' | 'W' | 'B' | 'E'))
                if k.modifiers.is_empty() || k.modifiers == KeyModifiers::SHIFT =>
            {
                let motion = match c.to_ascii_lowercase() {
                    'w' => copy::Word::Next,
                    'b' => copy::Word::Back,
                    _ => copy::Word::End,
                };
                let big = c.is_uppercase();
                let (pane, row) = (mode.pane, mode.at.0);
                let line = self.copy_line(pane, row);
                if let Some(mode) = self.copy.as_mut() {
                    mode.at.1 = copy::word(&line, mode.at.1, motion, big);
                }
            }
            // A blank line is the boundary, which in a terminal is the gap
            // between one command's output and the next.
            KeyCode::Char(c @ ('{' | '}')) => {
                let pane = mode.pane;
                let lines = self.copy_lines(pane);
                if let Some(mode) = self.copy.as_mut() {
                    mode.at.0 = copy::paragraph(&lines, mode.at.0, c == '}');
                }
            }
            KeyCode::PageUp => {
                self.session.scroll_focused(rows as isize / 2);
            }
            KeyCode::PageDown => {
                self.session.scroll_focused(-(rows as isize) / 2);
            }
            KeyCode::Char('g') => {
                self.session.scroll_focused(isize::MAX / 2);
            }
            KeyCode::Char('G') => {
                self.session.unscroll_focused();
                mode.at.0 = rows.saturating_sub(1);
            }
            _ => {}
        }
    }

    /// A key while a search query is being typed.
    ///
    /// Enter takes you to the first match and leaves the query standing, so `n`
    /// repeats it. Escape abandons the search and puts the cursor back where it
    /// was, which is what makes trying one free.
    fn search_key(&mut self, k: KeyEvent) {
        let Some(mode) = self.copy.as_mut() else {
            return;
        };
        let Some(search) = mode.search.as_mut() else {
            return;
        };
        match k.code {
            KeyCode::Esc => {
                mode.search = None;
                self.note("read: v select · y copy · esc");
            }
            KeyCode::Enter => {
                search.typing = false;
                let (search, pane, from) = (search.clone(), mode.pane, mode.at);
                let forward = search.forward;
                let lines = self.copy_lines(pane);
                match search.find(&lines, from, forward) {
                    Some(at) => {
                        if let Some(mode) = self.copy.as_mut() {
                            mode.at = at;
                        }
                        self.note(&format!("/{} · n next · N back", search.query));
                    }
                    None => self.note(&format!("no {:?} on this screen", search.query)),
                }
            }
            KeyCode::Backspace => {
                search.query.pop();
                let line = format!("{}{}", if search.forward { '/' } else { '?' }, search.query);
                self.note(&line);
            }
            KeyCode::Char(c) => {
                search.query.push(c);
                let line = format!("{}{}", if search.forward { '/' } else { '?' }, search.query);
                self.note(&line);
            }
            _ => {}
        }
    }

    /// One row of the pane being read, as text.
    fn copy_line(&self, pane: crate::mux::PaneId, row: u16) -> String {
        self.copy_lines(pane)
            .get(row as usize)
            .cloned()
            .unwrap_or_default()
    }

    /// Every row of the pane being read, as text.
    ///
    /// The visible screen, which is what copy mode is looking at: scrolling
    /// moves the window vt100 already keeps, so a pane read from the past
    /// answers here exactly as one at the bottom does.
    fn copy_lines(&self, pane: crate::mux::PaneId) -> Vec<String> {
        let Some(p) = self
            .session
            .focused_workspace()
            .and_then(|ws| ws.pane(pane))
        else {
            return Vec::new();
        };
        let Ok(term) = p.term.lock() else {
            return Vec::new();
        };
        let screen = term.screen();
        let (rows, cols) = screen.size();
        (0..rows)
            .map(|r| screen.contents_between(r, 0, r, cols))
            .collect()
    }

    /// Take what is selected, and give it to the end with a clipboard.
    fn take_selection(&mut self) {
        let Some(mode) = self.copy.as_ref() else {
            return;
        };
        let Some(span) = mode.span() else {
            return self.note("nothing selected");
        };
        let text = self
            .session
            .focused_workspace()
            .and_then(|ws| ws.pane(mode.pane))
            .and_then(|p| p.term.lock().ok().map(|t| copy::text(t.screen(), span)));
        let Some(text) = text.filter(|t| !t.is_empty()) else {
            return self.note("nothing selected");
        };
        let lines = text.lines().count();
        self.clip(text);
        self.copy = None;
        self.session.unscroll_focused();
        self.note(&format!(
            "copied {lines} {}",
            if lines == 1 { "line" } else { "lines" }
        ));
    }

    /// Put text on the clipboard of the machine somebody is sitting at.
    ///
    /// Sent to the client for the same reason an alert is: `pbcopy` in the
    /// server would put a remote session's selection on the build box, where
    /// nothing can paste it.
    fn clip(&mut self, text: String) {
        if self.views.is_empty() {
            return clipboard::put(&self.cfg.clipboard, &text);
        }
        // Every client: the selection was made on one of them, but which one is
        // not something this knows, and a clipboard that is sometimes empty is
        // worse than one that is sometimes shared.
        for view in &mut self.views {
            let _ = wire::send_json(&mut view.out, wire::Kind::Clip, &text);
        }
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
        // Typing is a statement about the live screen, so it brings you back to
        // it. Sending a keystroke to a program whose output you cannot see is
        // the kind of thing you only find out about afterwards.
        self.session.unscroll_focused();
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
        // The one that asked, and only that one. Detaching a laptop should not
        // take the monitor with it.
        let me = self.acting;
        match me.and_then(|id| self.views.iter().position(|v| v.id == id)) {
            Some(i) => {
                let mut view = self.views.remove(i);
                let _ = wire::send_json(&mut view.out, wire::Kind::Bye, &"detached");
                self.resize_panes();
            }
            None => self.note("nothing attached"),
        }
    }

    fn command(&mut self, k: KeyEvent) {
        // Pressing the prefix twice sends a literal one through.
        if k.code == KeyCode::Char(' ') && k.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(p) = self.session.active_pane_mut() {
                p.write(&[0]);
            }
            return;
        }

        // The named actions first, so a rebind reaches everything. What is left
        // after them is the things that are not actions: a layout's own key,
        // and the two spellings of split that are shapes rather than words.
        let pressed = match k.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Tab => "tab".into(),
            KeyCode::BackTab => "shift-tab".into(),
            KeyCode::Down => "down".into(),
            KeyCode::Up => "up".into(),
            _ => String::new(),
        };
        if let Some(action) = self.keys.action(&pressed) {
            return self.perform(action);
        }

        let (rows, cols) = (self.content.height, self.content.width);
        match k.code {
            // `|` and `-` draw the split they make, which is worth keeping
            // beside the letters even though nothing would break without them.
            KeyCode::Char('|') => self.session.split(Dir::Cols, rows, cols),
            KeyCode::Char('-') => self.session.split(Dir::Rows, rows, cols),
            KeyCode::Tab | KeyCode::Down => self.session.step_workspace(1),
            KeyCode::BackTab | KeyCode::Up => self.session.step_workspace(-1),
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

    /// Do one of the named things.
    ///
    /// The palette and the keyboard both come through here, so a thing that
    /// works one way cannot fail to work the other -- which is the failure the
    /// hit map already avoids for the pointer.
    fn perform(&mut self, action: action::Action) {
        use action::Action as A;
        if let Some(why) = action.why_not(&self.session) {
            return self.note(why);
        }
        let (rows, cols) = (self.content.height, self.content.width);
        match action {
            A::Detach => self.detach(),
            A::Quit => self.quit = true,
            A::NewWorkspace => {
                if let Focus::Ws { p, .. } = self.session.focus {
                    self.session.new_workspace(p, rows, cols);
                }
            }
            A::OpenProject => self.open_picker(),
            A::NewAgent => self.new_agent(),
            A::NewWorktree => self.ask_for_worktree(),
            A::SplitCols => self.session.split(Dir::Cols, rows, cols),
            A::SplitRows => self.session.split(Dir::Rows, rows, cols),
            A::ClosePane => self.session.close_focused(),
            A::CyclePane => self.session.cycle_pane(),
            A::Zoom => {
                if let Some(ws) = self.session.focused_workspace_mut() {
                    ws.zoom();
                }
            }
            A::MovePaneBack => self.move_pane(-1),
            A::MovePaneOn => self.move_pane(1),
            A::Restart => {
                if !self.session.restart_focused(self.content) {
                    self.note("nothing to restart");
                }
            }
            A::NewTab => {
                if self.session.new_tab(rows, cols).is_none() {
                    self.note("no space focused");
                }
            }
            A::NextTab => {
                if let Some(ws) = self.session.focused_workspace_mut() {
                    ws.step_tab(1);
                }
            }
            A::PrevTab => {
                if let Some(ws) = self.session.focused_workspace_mut() {
                    ws.step_tab(-1);
                }
            }
            A::CloseTab => {
                if !self.session.close_tab() {
                    self.note("the last tab is the space");
                }
            }
            A::Read => self.start_copy(),
            A::Find => self.start_find(),
            A::NextSpace => self.session.step_workspace(1),
            A::PrevSpace => self.session.step_workspace(-1),
            A::Sidebar => self.sidebar = !self.sidebar,
            A::Nav => {
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
            A::Palette => self.open_palette(),
            A::Release => match self.session.release_hold() {
                true => self.note("naming released"),
                false => self.note("not held"),
            },
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
        // A space in *this* checkout. Opening a worktree of a project that is
        // already open must not land you in the repository proper, which is
        // the one directory you did not ask for.
        let here = self.session.projects[p]
            .workspaces
            .iter()
            .position(|w| w.at == path);
        match here {
            Some(w) => self.session.focus = Focus::Ws { p, w },
            None => {
                self.session.new_workspace_at(p, path, rows, cols);
            }
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

        // The wheel over a pane reads its scrollback, unless the program inside
        // asked for mouse events -- `less` and `nvim` scroll themselves, and
        // taking the wheel off them would be worse than not having this.
        let wheel = match m.kind {
            MouseEventKind::ScrollUp => 1,
            MouseEventKind::ScrollDown => -1,
            _ => 0,
        };
        if wheel != 0 && self.picker.is_none() && !self.pane_wants_mouse() {
            self.focus_pane_under(&m);
            let back = self.session.scroll_focused(wheel * 3);
            // Scrolling up is a way into reading, and reaching the bottom is
            // the way out. Nobody wants a mode to leave after a wheel gesture.
            match back {
                0 => self.copy = None,
                _ if self.copy.is_none() => self.start_reading(),
                _ => {}
            }
            return;
        }

        // Selecting with the pointer, which is the gesture everybody already
        // knows. Only in a pane that is not itself listening for the mouse.
        if self.picker.is_none()
            && !self.pane_wants_mouse()
            && self.select_with_pointer(&m) == Some(true)
        {
            return;
        }

        // Anything else in the content area belongs to the pane under it.
        if self.picker.is_none() {
            self.send_mouse(&m);
        }
    }

    /// Whether the program in the focused pane asked for mouse events.
    ///
    /// A pager or an editor does its own scrolling and its own selection, and
    /// taking either off it would be worse than not having them at all.
    fn pane_wants_mouse(&self) -> bool {
        self.session
            .focused_workspace()
            .and_then(|ws| ws.active_pane())
            .and_then(|p| {
                p.term
                    .lock()
                    .ok()
                    .map(|t| t.screen().mouse_protocol_mode() != vt100::MouseProtocolMode::None)
            })
            .unwrap_or(false)
    }

    /// Reading without having asked for it, which is what a wheel gesture is.
    fn start_reading(&mut self) {
        let Some(ws) = self.session.focused_workspace() else {
            return;
        };
        self.copy = Some(copy::Mode::new(ws.focus(), (0, 0)));
    }

    /// Focus whatever pane the pointer is over, so the wheel reads that one.
    fn focus_pane_under(&mut self, m: &MouseEvent) {
        let rects = self.visible_rects();
        let Some(index) = rects.iter().position(|r| {
            m.column >= r.x && m.column < r.right() && m.row >= r.y && m.row < r.bottom()
        }) else {
            return;
        };
        if let Some(ws) = self.session.focused_workspace_mut()
            && let Some(&id) = ws.tree().leaves().get(index)
        {
            ws.set_focus(id);
        }
    }

    /// Drag to select, release to copy. `None` when the pointer was not over a
    /// pane at all, so the caller can carry on with whatever else it means.
    fn select_with_pointer(&mut self, m: &MouseEvent) -> Option<bool> {
        let rects = self.visible_rects();
        let index = rects.iter().position(|r| {
            m.column >= r.x && m.column < r.right() && m.row >= r.y && m.row < r.bottom()
        })?;
        let r = rects[index];
        let labelled = self.visible_pane(index).is_some_and(|p| p.label.is_some());
        let inner = mux::session::content_of(self.cfg.ui.ruled(labelled), r);
        if m.row < inner.y {
            return Some(false);
        }
        let at = (m.row - inner.y, m.column - inner.x);

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus_pane_under(m);
                let Some(ws) = self.session.focused_workspace() else {
                    return Some(false);
                };
                // Started but not anchored: a click is a click until it moves,
                // and anchoring here would make every click a one-cell
                // selection that swallows the click the pane wanted.
                let mut mode = copy::Mode::new(ws.focus(), at);
                mode.dragging = true;
                self.copy = Some(mode);
                Some(false)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let mode = self.copy.as_mut()?;
                if !mode.dragging {
                    return Some(false);
                }
                mode.anchor.get_or_insert(mode.at);
                mode.at = at;
                Some(true)
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let mode = self.copy.as_mut()?;
                if !mode.dragging {
                    return Some(false);
                }
                mode.dragging = false;
                match mode.anchor.is_some() {
                    // A drag that ended: take it, which is what selecting is
                    // for. A click that never moved is not a selection.
                    true => {
                        self.take_selection();
                        Some(true)
                    }
                    false => {
                        self.copy = None;
                        Some(false)
                    }
                }
            }
            _ => Some(false),
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
            && let Some(&id) = ws.tree().leaves().get(index)
        {
            ws.set_focus(id);
        }

        let ui = self.cfg.ui.clone();
        let Some(pane) = self.visible_pane_mut(index) else {
            return;
        };

        // A ruled pane's terminal starts below the rule, so the event has to be
        // measured from there. Measuring from the rectangle would put every
        // click one row low -- selecting the line above the one you pointed at.
        let inner = mux::session::content_of(ui.ruled(pane.label.is_some()), r);
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
    reading: Option<&copy::Mode>,
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

        // A ruled pane gives its top row to a rule carrying the label, which is
        // how a five-pane board says which panel is which. With no label the
        // rule still says which pane has the keyboard, which is what tells two
        // shells side by side apart.
        let ruled = session.ui.ruled(pane.label.is_some());
        let inner = mux::session::content_of(ruled, r);
        if ruled {
            rule(
                buf,
                Rect { height: 1, ..r },
                g,
                pane.label.as_deref().unwrap_or_default(),
                pane.exit.as_deref(),
                id == ws.focus(),
            );
        }
        if let Ok(t) = pane.term.lock() {
            // A stopped pane keeps its last output, dimmed. The output is
            // usually why the panel was there.
            let span = reading.filter(|m| m.pane == id).and_then(|m| m.span());
            ui::pane::paint(t.screen(), inner, buf, id != ws.focus() || pane.dead, span);
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
