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

//! Does a session outlive the terminal it was started from?
//!
//! Everything here needs two clients, or a client that dies, so it cannot be
//! done in the smoke harness — which runs one process and ends with it. Each
//! test gets a session of its own, because they would otherwise all attach to
//! whichever server started first.

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ROWS: u16 = 24;
const COLS: u16 = 80;
const READY: &str = "+ workspace";
const START: Duration = Duration::from_secs(30);

/// One configuration directory for the whole run, away from the checkout.
fn config_home() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dirk-tests-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A session name that takes its session with it.
///
/// Every test used to end with `quit(&session)`, which a failing assertion
/// never reaches -- so a red run left one daemon per failed test on the
/// machine, invisible unless you thought to run `dirk session list`. A panic
/// unwinds, and unwinding runs `Drop`, so the end of a session belongs here
/// rather than at the end of a function that may not get there.
struct Live(String);

impl std::ops::Deref for Live {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        // Over the socket rather than by attaching: this may be running inside
        // a panic, and taking a terminal over on the way out of one is a way to
        // lose the message that said what went wrong.
        let _ = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
            .args(["--session", &self.0, "session", "quit"])
            .env("XDG_CONFIG_HOME", config_home())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let socket = socket_path(&self.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while socket.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = std::fs::remove_file(&socket);
    }
}

/// End a session on purpose, mid-test, and wait for it to be gone.
///
/// Over the socket rather than by attaching and pressing `q`: taking a terminal
/// over in order to end a session is a lot of machinery for something that is
/// now one command.
fn end(session: &str) {
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--session", session, "session", "quit"])
        .env("XDG_CONFIG_HOME", config_home())
        .output();
    let socket = socket_path(session);
    let deadline = Instant::now() + Duration::from_secs(10);
    while socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn socket_path(session: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join(format!("dirk-{}", unsafe { libc::getuid() }))
        .join(format!("{session}.sock"))
}

fn unique(kind: &str) -> Live {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    Live(format!(
        "test-{kind}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// One client attached to a named session.
struct Client {
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    screen: Arc<Mutex<vt100::Parser>>,
    cols: u16,
    rows: u16,
}

impl Client {
    fn attach(session: &str) -> Self {
        Self::attach_sized(session, COLS, ROWS)
    }

    /// A client with a configuration directory of its own.
    ///
    /// Sound is performed by whichever end has the speakers, so testing it
    /// means giving this end a configuration the rest of the run does not
    /// share.
    fn with_config(session: &str, dir: &std::path::Path) -> Self {
        let dir = dir.to_path_buf();
        Self::spawn(session, COLS, ROWS, &move |cmd| {
            cmd.env("XDG_CONFIG_HOME", &dir);
        })
    }

    /// The way `--remote` reaches a session, with a stand-in for ssh.
    ///
    /// Not a network: what is being tested is the transport -- a pipe pair
    /// instead of a socket, a relay on the far end, and a client that survives
    /// losing one -- and a test that needed a second machine would be a test
    /// nobody runs.
    fn over(session: &str, ssh: &std::path::Path) -> Self {
        Self::spawn(session, COLS, ROWS, &|cmd| {
            cmd.args(["--remote", "somewhere"]);
            cmd.env("DIRK_SSH", ssh);
            cmd.env("DIRK_REMOTE", env!("CARGO_BIN_EXE_dirk"));
        })
    }

    fn attach_sized(session: &str, cols: u16, rows: u16) -> Self {
        Self::spawn(session, cols, rows, &|_| {})
    }

    fn spawn(session: &str, cols: u16, rows: u16, extra: &dyn Fn(&mut CommandBuilder)) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_dirk"));
        cmd.args(["--session", session]);
        cmd.cwd(env!("CARGO_MANIFEST_DIR"));
        cmd.env("TERM", "xterm-256color");
        cmd.env("SHELL", "/bin/sh");
        // Its own configuration directory. Sessions are written down under it,
        // and pointing this at the checkout would leave saved sessions in the
        // repository.
        cmd.env("XDG_CONFIG_HOME", config_home());
        for var in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
        ] {
            cmd.env_remove(var);
        }

        extra(&mut cmd);

        let child = pair.slave.spawn_command(cmd).expect("spawn dirk");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("reader");
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

        Self {
            writer: pair.master.take_writer().expect("writer"),
            child,
            screen,
            cols,
            rows,
        }
    }

    /// Wait for the client itself to end, and say how.
    fn ended(&mut self, within: Duration) -> Option<u32> {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Some(status.exit_code());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }

    fn rows(&self) -> Vec<String> {
        let s = self.screen.lock().unwrap();
        (0..self.rows)
            .map(|r| {
                (0..self.cols)
                    .map(|c| match s.screen().cell(r, c) {
                        Some(cell) if !cell.contents().is_empty() => cell.contents().to_string(),
                        _ => " ".to_string(),
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn drawn(&self) -> String {
        self.rows().join("\n")
    }

    fn wait_until(&self, timeout: Duration, done: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn wait_for(&self, needle: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.rows().iter().any(|r| r.contains(needle)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    fn click(&mut self, col: u16, row: u16) {
        let (c, r) = (col + 1, row + 1);
        self.send(format!("\x1b[<0;{c};{r}M").as_bytes());
        self.send(format!("\x1b[<0;{c};{r}m").as_bytes());
        std::thread::sleep(Duration::from_millis(200));
    }

    fn send(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// End this client the way closing a terminal does.
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn the_session_outlives_the_terminal_it_was_started_from() {
    let session = unique("outlives");

    let mut first = Client::attach(&session);
    assert!(
        first.wait_for(READY, START),
        "never started\n{}",
        first.drawn()
    );

    first.send(b"printf 'zz%s' ALIVE\r");
    assert!(
        first.wait_for("zzALIVE", Duration::from_secs(10)),
        "no echo\n{}",
        first.drawn()
    );

    // The terminal goes. Everything in the session should not.
    first.kill();
    std::thread::sleep(Duration::from_millis(500));

    let second = Client::attach(&session);
    assert!(
        second.wait_for("zzALIVE", START),
        "the shell and its output did not survive the terminal\n{}",
        second.drawn()
    );
    drop(second);
}

#[test]
fn reattaching_shows_what_happened_while_nobody_was_watching() {
    let session = unique("meanwhile");

    let mut first = Client::attach(&session);
    assert!(
        first.wait_for(READY, START),
        "never started\n{}",
        first.drawn()
    );

    // Output that will land after the terminal is gone.
    first.send(b"(sleep 2; printf 'zz%s' LATER) &\r");
    std::thread::sleep(Duration::from_millis(300));
    first.kill();

    std::thread::sleep(Duration::from_secs(3));
    let second = Client::attach(&session);
    assert!(
        second.wait_for("zzLATER", START),
        "work done with nobody attached was lost\n{}",
        second.drawn()
    );
    drop(second);
}

#[test]
fn two_clients_watch_the_same_session_at_once() {
    // The case two clients exist for: a laptop and a monitor showing different
    // parts of one session. A second one used to take the session over and tell
    // the first why, which is honest and is not what anybody wanted.
    let session = unique("both");

    let first = Client::attach(&session);
    assert!(
        first.wait_for(READY, START),
        "never started\n{}",
        first.drawn()
    );

    let second = Client::attach(&session);
    assert!(
        second.wait_for(READY, START),
        "the second client never drew\n{}",
        second.drawn()
    );

    // The first is still there, and still being drawn for.
    assert!(
        !first.drawn().contains("taken over"),
        "the first was thrown off\n{}",
        first.drawn()
    );
    let (ok, _) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let id = first_id(&list);
    let (ok, _) = ask(&session, &["workspace", "rename", &id, "zzBOTH"]);
    assert!(ok, "rename failed");
    assert!(
        first.wait_for("zzBOTH", START),
        "the first client stopped being drawn for\n{}",
        first.drawn()
    );
    assert!(
        second.wait_for("zzBOTH", START),
        "the second client stopped being drawn for\n{}",
        second.drawn()
    );

    // And the session says how many are watching.
    let (ok, info) = ask(&session, &["session", "info"]);
    assert!(ok, "session info failed");
    assert!(info.contains("\"clients\": 2"), "it counted wrong: {info}");

    drop(second);
    drop(first);
}

#[test]
fn one_client_leaving_does_not_disturb_another() {
    // A detach used to be the end of the only view there was. With two of them
    // it has to be the end of exactly one.
    let session = unique("survivor");

    let first = Client::attach(&session);
    assert!(first.wait_for(READY, START), "never started");
    let mut second = Client::attach(&session);
    assert!(second.wait_for(READY, START), "the second never drew");

    drop(first);
    std::thread::sleep(Duration::from_millis(800));

    // The survivor still works, and is still the one being drawn for.
    second.send(b"printf 'zz%s' AFTER\r");
    assert!(
        second.wait_for("zzAFTER", Duration::from_secs(10)),
        "the client that stayed was frozen\n{}",
        second.drawn()
    );
    let (ok, info) = ask(&session, &["session", "info"]);
    assert!(ok, "session info failed");
    assert!(info.contains("\"clients\": 1"), "it counted wrong: {info}");
    drop(second);
}

#[test]
fn each_client_looks_where_it_is_looking() {
    // Focus stopped being a property of the session the moment there could be
    // two of them, which is the whole reason this was not a transport change.
    let session = unique("apart");

    let first = Client::attach(&session);
    assert!(first.wait_for(READY, START), "never started");
    // Three, so a step in one client cannot land on the same one the other is
    // already looking at and pass by accident.
    for _ in 0..2 {
        let (ok, _) = ask(&session, &["workspace", "create"]);
        assert!(ok, "workspace create failed");
    }

    let mut second = Client::attach(&session);
    assert!(second.wait_for(READY, START), "the second never drew");
    // The nav counts the spaces; the rail no longer does, because a number
    // that is always there is a middle that can never be empty.
    assert!(
        second.wait_until(START, |c| c.rows().iter().any(|r| {
            // The sidebar's columns only. Past them is a pane, and on a runner
            // that pane has a shell prompt in it -- so the row ends in `$` and
            // a count at the end of the *row* is not the count at the end of
            // the section header.
            let nav: String = r.chars().take(34).collect();
            nav.contains("spaces") && nav.trim_end().ends_with('3')
        })),
        "not three spaces yet\n{}",
        second.drawn()
    );

    // One of them moves, with a key rather than over the socket -- the socket
    // has no screen of its own and moves them all, which is a different thing.
    second.send(&[0]);
    second.send(b"j");
    std::thread::sleep(Duration::from_millis(800));

    // The rail names where you are, so the two screens disagree about where
    // that is -- which is the point.
    let bar = |c: &Client| c.rows().last().cloned().unwrap_or_default();
    assert_ne!(
        bar(&first),
        bar(&second),
        "both clients are looking at the same thing:\n{}\n{}",
        bar(&first),
        bar(&second)
    );

    drop(second);
    drop(first);
}

#[test]
fn a_session_uses_the_whole_terminal_it_is_shown_on() {
    // The server has no terminal of its own. Asking a backend for "the size"
    // there answers with the process's, which fails and falls back to 80x24 --
    // so every client on anything larger got an 80x24 dirk, and the panes
    // inside it were 80x24 too. A suite that only ever opens 80x24 terminals is
    // exactly the one that would not notice.
    let session = unique("big");
    let client = Client::attach_sized(&session, 120, 40);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    let rows = client.rows();
    let last = rows.iter().rposition(|r| !r.trim().is_empty()).unwrap_or(0);
    assert!(
        last > 24,
        "nothing was drawn below row 24 of a 40-row terminal: the size did not survive\n{}",
        client.drawn()
    );
    // And the rail runs the whole width.
    let widest = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    assert!(
        widest > 80,
        "nothing was drawn past column 80 of a 120-column terminal"
    );
    drop(client);
}

/// The first `"id": "..."` in an answer.
///
/// Not "the first word starting with w": `worktree` is a key, and matching it
/// meant renaming a workspace called `worktree` that does not exist.
fn first_id(json: &str) -> String {
    let at = json.find("\"id\"").expect("an id in the answer");
    let rest = &json[at + 4..];
    let open = rest.find('"').expect("a value");
    let rest = &rest[open + 1..];
    rest[..rest.find('"').expect("a closing quote")].to_string()
}

/// Ask a running session something, the way a shell or an agent would.
fn ask(session: &str, args: &[&str]) -> (bool, String) {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"));
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
    ] {
        cmd.env_remove(var);
    }
    let out = cmd
        .args(["--session", session])
        .args(args)
        .env("XDG_CONFIG_HOME", config_home())
        .output()
        .expect("run dirk");
    // Both, because a failure says why on stderr and a caller checking only
    // stdout gets an empty string and no idea what went wrong.
    let said = match out.status.success() {
        true => String::from_utf8_lossy(&out.stdout).into_owned(),
        false => String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    (out.status.success(), said)
}

#[test]
fn a_session_answers_for_itself() {
    // The difference between a multiplexer agents happen to run in and one they
    // can work in. Driven the way a caller would: ask what is there, act on an
    // id from the answer, and read back what happened.
    let session = unique("api");
    let client = Client::attach(&session);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let id = first_id(&panes);

    // Ids come back to dirk in the next command.
    let (ok, split) = ask(&session, &["pane", "split", &id, "rows"]);
    assert!(ok, "split failed: {split}");
    assert!(split.contains(":p"), "the new pane was not named: {split}");

    let (ok, _) = ask(&session, &["pane", "run", &id, "printf zzASKED"]);
    assert!(ok, "pane run failed");

    // Read it back from the pane rather than from the screen: that is the path
    // an agent looking at a neighbour would take.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw = false;
    while Instant::now() < deadline && !saw {
        saw = ask(&session, &["pane", "read", &id, "10"])
            .1
            .contains("zzASKED");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(saw, "what was typed never came back out");

    // A failure is structured, not prose on stdout.
    let (ok, _) = ask(&session, &["pane", "read", "p99999"]);
    assert!(!ok, "reading a pane that does not exist should fail");

    drop(client);
}

#[test]
fn a_command_does_not_conjure_a_session_to_answer_it() {
    // `dirk pane list` with nothing running should say so, not start a session
    // in order to have something to list.
    let session = unique("absent");
    let (ok, _) = ask(&session, &["pane", "list"]);
    assert!(!ok, "a command started a session");
    let socket = std::env::temp_dir()
        .join(format!("dirk-{}", unsafe { libc::getuid() }))
        .join(format!("{session}.sock"));
    assert!(!socket.exists(), "a command left a session behind");
}

#[test]
fn sessions_are_listed_with_whether_you_can_attach_to_one() {
    // Answered by connecting, not by the directory listing: a socket outliving
    // its server is the ordinary state after a crash, and listing those as
    // sessions would be listing things you cannot attach to.
    let session = unique("listed");
    let client = Client::attach(&session);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["session", "list"])
        .output()
        .expect("run dirk");
    let listed = String::from_utf8_lossy(&out.stdout).into_owned();
    // A client is attached, which is a stronger thing to know than that a
    // server is up — and the only one of the two that needs asking.
    assert!(
        listed
            .lines()
            .any(|l| l.starts_with(&*session) && l.contains("attached")),
        "the session with a client attached was not listed as attached:\n{listed}"
    );

    drop(client);
    end(&session);

    // And once it is gone it is not offered as something to attach to.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["session", "list"])
        .output()
        .expect("run dirk");
    let listed = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        !listed
            .lines()
            .any(|l| l.starts_with(&*session) && (l.contains("running") || l.contains("attached"))),
        "a session that has ended is still offered as one to attach to:\n{listed}"
    );
}

#[test]
fn a_reload_does_not_disturb_what_is_running() {
    let session = unique("reload");
    let mut client = Client::attach(&session);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    client.send(b"printf 'zz%s' BEFORE\r");
    assert!(
        client.wait_for("zzBEFORE", Duration::from_secs(10)),
        "no echo\n{}",
        client.drawn()
    );

    let (ok, _) = ask(&session, &["session", "reload"]);
    assert!(ok, "reload failed");

    // The shell is still there, and still the one that was there.
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        client.rows().iter().any(|r| r.contains("zzBEFORE")),
        "a reload took the session with it\n{}",
        client.drawn()
    );
    client.send(b"printf 'zz%s' AFTER\r");
    assert!(
        client.wait_for("zzAFTER", Duration::from_secs(10)),
        "the pane stopped taking input after a reload\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_command_from_inside_a_pane_reaches_the_session_holding_it() {
    // Every pane is told which session it belongs to. Without reading that
    // back, a command from inside one went to `default` -- and pane ids are
    // per-session counters, so `--current` could name a live pane in another
    // session and type into a stranger's shell.
    let session = unique("routed");
    let client = Client::attach(&session);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    // No `--session`: the environment is what a pane has.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["pane", "list"])
        .env("DIRK_SESSION", &*session)
        .env("XDG_CONFIG_HOME", config_home())
        .output()
        .expect("run dirk");
    assert!(
        out.status.success(),
        "a command from inside a pane did not reach its session: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(":p"),
        "no panes in the answer"
    );

    drop(client);
}

#[test]
fn the_shape_of_a_session_comes_back_after_it_has_ended() {
    // Not the panes: a pane is a process, and a screenful of text with nothing
    // behind it is worse than an empty pane because it looks like something you
    // can type into. What comes back is what a human arranged.
    let session = unique("shape");

    let client = Client::attach(&session);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let id = first_id(&list);
    let (ok, _) = ask(
        &session,
        &["workspace", "rename", &id, "Named", "by", "hand"],
    );
    assert!(ok, "rename failed");
    assert!(
        client.wait_for("Named by hand", Duration::from_secs(10)),
        "the rename never showed\n{}",
        client.drawn()
    );

    // End the session entirely, server and all.
    drop(client);
    end(&session);

    // And start it again from nothing.
    let second = Client::attach(&session);
    assert!(
        second.wait_for(READY, START),
        "did not come back\n{}",
        second.drawn()
    );
    assert!(
        second.wait_for("Named by hand", START),
        "the name did not survive the session ending\n{}",
        second.drawn()
    );
    drop(second);
}

#[test]
fn an_answer_nobody_is_reading_ends_quietly() {
    // A reader that is gone before anything is written, which is what every
    // `| head`, closed pager and `grep -q` eventually looks like. Every other
    // program on the system ends this in silence; `print!` ends it in a
    // backtrace.
    for args in [
        "--skill",
        "--help",
        "session list",
        "--session no-such-session pane list",
    ] {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{} {args} | true", env!("CARGO_BIN_EXE_dirk")))
            .env("XDG_CONFIG_HOME", config_home())
            .output()
            .expect("run it through a pipe");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            !said.contains("panicked"),
            "`dirk {args}` panicked into a closed pipe:\n{said}"
        );
    }
}

// ── Over a link that is not a socket ────────────────────────────────────

/// A stand-in for ssh that lets one link through and refuses every one after.
///
/// What a host that has gone looks like once you were already talking to it,
/// and the case where a link is made -- ssh starts, the pipes are there -- and
/// still delivers nothing.
fn fake_ssh_once(name: &str) -> std::path::PathBuf {
    let mark = config_home().join(format!("mark-{name}"));
    let body = format!(
        "#!/bin/sh\nshift 3\n\
         if [ -f {mark} ]; then exit 0; fi\n\
         : > {mark}\n\
         exec \"$@\"\n",
        mark = mark.display()
    );
    let _ = std::fs::remove_file(&mark);
    script(name, &body)
}

/// A stand-in for ssh: drops `-T -- <target>` and runs the rest here.
///
/// `flaky` makes the first link die two seconds in, which is what a laptop lid
/// looks like from this side.
fn fake_ssh(name: &str, flaky: bool) -> std::path::PathBuf {
    let body = match flaky {
        false => "#!/bin/sh\nshift 3\nexec \"$@\"\n".to_string(),
        // Killed rather than backgrounded: a POSIX shell gives an asynchronous
        // list /dev/null for stdin, and a relay with no input has nothing to
        // relay. The watchdog kills the script, which `exec` has made the relay.
        true => format!(
            "#!/bin/sh\nshift 3\n\
             if [ -f {mark} ]; then exec \"$@\"; fi\n\
             : > {mark}\n\
             ( sleep 2; kill $$ 2>/dev/null ) &\n\
             exec \"$@\"\n",
            mark = config_home().join(format!("mark-{name}")).display()
        ),
    };
    let _ = std::fs::remove_file(config_home().join(format!("mark-{name}")));
    script(name, &body)
}

fn script(name: &str, body: &str) -> std::path::PathBuf {
    let path = config_home().join(format!("ssh-{name}"));
    std::fs::write(&path, body).expect("write the stand-in");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path
}

#[test]
fn a_session_reached_over_a_pipe_pair_draws_the_same_thing() {
    // The client does not know what it is talking to. That is the whole claim
    // of the remote attach: a transport, not a second implementation.
    let session = unique("remote");
    let client = Client::over(&session, &fake_ssh("plain", false));
    assert!(
        client.wait_for(READY, START),
        "nothing came back over the relay\n{}",
        client.drawn()
    );
    assert!(
        client.wait_for("spaces", START),
        "the frame is not a dirk frame\n{}",
        client.drawn()
    );
    drop(client);
}

#[test]
fn a_dropped_link_comes_back_to_the_same_session() {
    // The session is a daemon and the link is not the session. Losing one
    // should cost you the seconds it takes to make another and nothing else.
    let session = unique("dropped");
    let client = Client::over(&session, &fake_ssh("flaky", true));
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    // Past the two seconds the stand-in allows the first link.
    std::thread::sleep(Duration::from_secs(4));

    // Something the old frame cannot contain, so seeing it means a new link
    // reached the session that was there before -- not a repaint of the last
    // thing painted.
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "the session did not outlive the link: {list}");
    let id = first_id(&list);
    let (ok, _) = ask(&session, &["workspace", "rename", &id, "Still", "here"]);
    assert!(ok, "rename failed");
    assert!(
        client.wait_for("Still here", START),
        "the link never came back\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_link_that_never_worked_is_not_waited_for() {
    // Nothing to wait for. A connection that never delivered a frame is a
    // configuration problem wearing a network problem's clothes, and two
    // minutes of patience spent on one is two minutes of a blank screen.
    let ssh = config_home().join("ssh-dead");
    std::fs::write(
        &ssh,
        "#!/bin/sh\necho 'ssh: no route to host' >&2\nexit 255\n",
    )
    .expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    let mut client = Client::over(&unique("dead"), &ssh);
    let code = client.ended(Duration::from_secs(15));
    assert_eq!(
        code,
        Some(1),
        "a host that does not answer should end the client, and say so:\n{}",
        client.drawn()
    );
}

#[test]
fn a_session_that_dies_reaches_the_client_as_a_lost_link() {
    // The relay copies in two directions and only one of them decides when it
    // is over. If the far session going away does not end the relay, the client
    // sees a link that is still open and a screen that has stopped changing.
    let session = unique("died");
    let client = Client::over(&session, &fake_ssh("dies", false));
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    let killed = std::process::Command::new("pkill")
        .args(["-f", &format!("dirk server --session {session}")])
        .status()
        .expect("pkill");
    assert!(killed.success(), "there was no server to kill");

    assert!(
        client.wait_for("reconnecting", Duration::from_secs(15)),
        "a dead session looked like a live one\n{}",
        client.drawn()
    );
    drop(client);
}

#[test]
fn remote_attaches_and_does_not_carry_a_command() {
    // `dirk --remote box pane list` used to answer about this machine, which is
    // a wrong answer rather than an error.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--remote", "somewhere", "pane", "list"])
        .env("XDG_CONFIG_HOME", config_home())
        .output()
        .expect("run it");
    assert!(!out.status.success(), "it answered about the wrong machine");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("does not carry a command"), "said: {said}");
}

#[test]
fn a_link_that_keeps_failing_is_tried_less_often_and_then_not_at_all() {
    // The count belongs to the run, not to one wait. A ladder that started over
    // every time a link was made -- and making one only means ssh started -- is
    // how a client ends up dialling a host that has gone for good once a
    // second, for ever, on a screen that says "reconnecting (1)" throughout.
    let session = unique("ladder");
    let client = Client::over(&session, &fake_ssh_once("once"));
    assert!(
        client.wait_for(READY, START),
        "the one good link never worked\n{}",
        client.drawn()
    );

    let killed = std::process::Command::new("pkill")
        .args(["-f", &format!("dirk server --session {session}")])
        .status()
        .expect("pkill");
    assert!(killed.success(), "there was no server to kill");

    // The first three waits are one, one and two seconds, so a client that is
    // counting reaches the fourth attempt in four and one that is not never
    // leaves the first. The allowance is for a loaded machine, not for the
    // ladder: the whole suite spawns a lot of processes.
    assert!(
        client.wait_for("reconnecting (4)", START),
        "the wait never got any longer\n{}",
        client.drawn()
    );
    drop(client);
}

#[test]
fn detaching_leaves_everything_running() {
    // The headline feature of 0.4 with no way to use it: `q` ended every shell
    // and every agent, and leaving without doing that meant closing the
    // terminal window.
    let session = unique("detach");
    let mut client = Client::attach(&session);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    // Something to come back to that is not the default.
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let id = first_id(&list);
    let (ok, _) = ask(&session, &["workspace", "rename", &id, "Still", "running"]);
    assert!(ok, "rename failed");
    assert!(
        client.wait_for("Still running", START),
        "the rename never showed"
    );

    client.send(&[0]); // the prefix
    client.send(b"d");

    assert!(
        client.ended(Duration::from_secs(10)).is_some(),
        "the client did not leave\n{}",
        client.drawn()
    );
    let (ok, after) = ask(&session, &["workspace", "list"]);
    assert!(ok, "the session went with the client: {after}");
    assert!(
        after.contains("Still running"),
        "the workspace did not survive detaching: {after}"
    );

    // And it is attachable again, which is the whole point.
    let second = Client::attach(&session);
    assert!(
        second.wait_for("Still running", START),
        "could not come back to it\n{}",
        second.drawn()
    );
    drop(second);
}

#[test]
fn the_bar_offers_both_ways_out_and_says_which_is_which() {
    // A single glyph cannot say whether it parks your work or ends it.
    let session = unique("exits");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    let drawn = client.drawn();
    assert!(
        drawn.contains("detach"),
        "no way out that keeps the work:\n{drawn}"
    );
    assert!(drawn.contains("quit"), "no way out that ends it:\n{drawn}");
    drop(client);
}

// ── What an agent says about itself ─────────────────────────────────────

#[test]
fn a_reported_state_outranks_what_the_screen_looks_like() {
    // Rank one. Everything else dirk has is inference -- argv, a title, prose
    // on a screen, silence -- and every one of those is it guessing at
    // something the agent already knows.
    let session = unique("said");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, out) = ask(&session, &["agent", "state", "blocked"]);
    assert!(ok, "the report was refused: {out}");
    assert!(
        client.wait_for("needs you", START),
        "a reported block did not reach the column\n{}",
        client.drawn()
    );

    // And it is a claim about a moment, not a lease: the next one replaces it.
    let (ok, _) = ask(&session, &["agent", "state", "working"]);
    assert!(ok, "the second report was refused");
    assert!(
        client.wait_until(START, |c| !c.drawn().contains("needs you")),
        "the first report outlived the second\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn output_contradicts_a_claim_that_nothing_is_happening() {
    // Without this a harness whose hook fires on stop but not on start sticks
    // on `done` while it grinds, which is worse than the guess it replaced.
    let session = unique("grind");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");
    let (ok, _) = ask(&session, &["agent", "state", "done"]);
    assert!(ok, "the report was refused");

    let (ok, _) = ask(&session, &["pane", "run", &pane, "printf", "'zzWORKING'"]);
    assert!(ok, "pane run failed");

    let (ok, after) = ask(&session, &["agent", "list"]);
    assert!(ok, "agent list failed: {after}");
    // The pane holds a shell rather than an agent, so it is not in that list at
    // all -- what matters is that the screen stopped saying the work is done.
    assert!(
        client.wait_until(START, |c| !c.drawn().contains("needs you")),
        "a pane producing output still read as finished\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_reported_block_does_not_outlive_the_answer_you_gave_it() {
    // A report is a claim about a moment, and the moment ends when the pane
    // says something the report did not account for. Without that, answering
    // the prompt leaves the workspace pinned to the top of the attention zone
    // until the next hook happens to fire.
    let session = unique("stuck");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");
    let (ok, _) = ask(&session, &["agent", "state", "blocked"]);
    assert!(ok, "the report was refused");
    assert!(
        client.wait_for("needs you", START),
        "the report never landed\n{}",
        client.drawn()
    );

    let (ok, _) = ask(&session, &["pane", "run", &pane, "printf 'zzON'"]);
    assert!(ok, "pane run failed");
    assert!(
        client.wait_until(START, |c| !c.drawn().contains("needs you")),
        "it was still waiting on somebody after the pane went back to work\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_state_nobody_defined_is_refused() {
    let session = unique("nostate");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    let (ok, _) = ask(&session, &["agent", "state", "pondering"]);
    assert!(!ok, "a state that does not exist was accepted");
    drop(client);
}

#[test]
fn starting_an_agent_refuses_a_pane_that_is_busy() {
    // Typing into somebody's editor is not recoverable by apologising, which is
    // the whole reason `available` is in the API.
    let session = unique("busy");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");

    // Something that holds the pane and is not a shell.
    // With the return, or the shell is still at a prompt and the pane is free.
    let (ok, _) = ask(&session, &["pane", "run", &pane, "sleep 30"]);
    assert!(ok, "pane run failed");
    std::thread::sleep(Duration::from_secs(3));

    let (ok, why) = ask(&session, &["agent", "start", "claude", &pane]);
    assert!(!ok, "it typed into a busy pane: {why}");
    assert!(why.contains("busy"), "said the wrong thing: {why}");

    drop(client);
}

#[test]
fn starting_an_agent_with_no_pane_means_the_one_you_are_in() {
    // The same rule as `agent state`: nothing said means here. A hook or a
    // shell command inside a pane should not have to look its own id up first.
    let session = unique("here");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    // A shell at a prompt, which is what a free pane is.
    std::thread::sleep(Duration::from_secs(2));

    let (ok, out) = ask(&session, &["agent", "start", "claude"]);
    assert!(ok, "it could not find the pane it was standing in: {out}");
    assert!(out.contains("claude"), "said the wrong thing: {out}");

    drop(client);
}

#[test]
fn an_agent_nobody_configured_is_refused_by_name() {
    let session = unique("nokind");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    let (ok, why) = ask(&session, &["agent", "start", "not-a-real-agent"]);
    assert!(!ok, "it tried to start something that does not exist");
    assert!(why.contains("no such agent"), "said the wrong thing: {why}");
    drop(client);
}

/// Every workspace id in an answer, in the order they appear.
fn ids(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(at) = rest.find("\"id\"") {
        rest = &rest[at + 4..];
        let Some(open) = rest.find('"') else { break };
        rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else { break };
        out.push(rest[..close].to_string());
        rest = &rest[close..];
    }
    out
}

/// The first value of a field in a JSON answer, without a parser.
fn first_field(json: &str, field: &str) -> String {
    let at = json
        .find(&format!("\"{field}\""))
        .unwrap_or_else(|| panic!("no {field} in the answer: {json}"));
    let rest = &json[at + field.len() + 2..];
    let open = rest.find('"').expect("a value");
    let rest = &rest[open + 1..];
    rest[..rest.find('"').expect("a close")].to_string()
}

// ── Being told, where you are ───────────────────────────────────────────

/// A configuration directory whose sound command leaves a file behind.
fn noisy(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = config_home().join(format!("noisy-{name}"));
    std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
    let rang = dir.join("rang");
    let _ = std::fs::remove_file(&rang);
    std::fs::write(
        dir.join("dirk").join("config.toml"),
        format!(
            "[sound]\nenabled = true\nblocked = [\"sh\", \"-c\", \"touch {}\"]\n",
            rang.display()
        ),
    )
    .expect("config");
    (dir, rang)
}

fn appears(path: &std::path::Path, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn the_noise_is_made_where_the_human_is() {
    // The session decides whether; the end with the speakers decides how. That
    // split is what makes a remote attach ring on the laptop somebody is
    // sitting at rather than on the build box -- and `notify.rs` used to run
    // `osascript` in the server, which is the same bug with a different output
    // device.
    let session = unique("noise");
    let (dir, rang) = noisy("here");
    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    // Somewhere else to be looking, because a workspace on your screen is one
    // you already know about.
    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let all = ids(&list);
    assert_eq!(all.len(), 2, "expected two workspaces: {list}");
    let (ok, _) = ask(&session, &["workspace", "focus", &all[1]]);
    assert!(ok, "workspace focus failed");

    let (ok, out) = ask(&session, &["agent", "state", "blocked", &all[0]]);
    assert!(ok, "the report was refused: {out}");
    assert!(
        appears(&rang, START),
        "nothing rang on the machine with the speakers\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn nothing_rings_about_the_workspace_you_are_looking_at() {
    // The rule that decides whether this is loved or muted within a week. If
    // the thing that just finished is the one on your screen, you know.
    let session = unique("quiet");
    let (dir, rang) = noisy("focused");
    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, out) = ask(&session, &["agent", "state", "blocked"]);
    assert!(ok, "the report was refused: {out}");
    // Long enough that it would have rung by now if it were going to.
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !rang.exists(),
        "it made a noise about the workspace on screen\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_session_ends_with_the_test_that_made_it() {
    // The guard, on its own. A failing assertion never reaches the end of a
    // test function, so a red run used to leave one daemon per failure on the
    // machine -- invisible unless you thought to run `dirk session list`.
    // Unwinding runs `Drop`, so this is where the end of a session belongs.
    let socket;
    {
        let session = unique("guard");
        socket = socket_path(&session);
        let client = Client::attach(&session);
        assert!(client.wait_for(READY, START), "never started");
        assert!(socket.exists(), "no socket to clean up");
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !socket.exists(),
        "the session outlived the test that made it"
    );
}

#[test]
fn a_socket_nothing_is_listening_on_can_be_swept_up() {
    // A server that dies without unbinding leaves its socket, and only a new
    // session of the same name ever removed one -- so `session list` filled up
    // with things you cannot attach to and there was no way to clear them.
    let dead = socket_path("test-stale-swept");
    if let Some(dir) = dead.parent() {
        std::fs::create_dir_all(dir).expect("socket dir");
    }
    std::fs::write(&dead, b"").expect("a socket nobody is listening on");

    // And one that is answering, which must survive.
    let live = unique("swept");
    let client = Client::attach(&live);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, out) = ask(&live, &["session", "prune"]);
    assert!(ok, "prune failed: {out}");
    assert!(!dead.exists(), "the stale socket was left behind:\n{out}");
    assert!(
        socket_path(&live).exists(),
        "it removed a socket somebody was listening on:\n{out}"
    );
    drop(client);
}

// ── Worktrees ───────────────────────────────────────────────────────────

/// A repository with one commit in it, somewhere disposable.
/// Where a worktree these tests made is allowed to be.
///
/// Every one of them ends up beside its repository, and the repository is under
/// the temp directory. A test whose dirk resolved the wrong checkout would make
/// one beside *this* repository instead -- which happened once, silently, and
/// left four branches and two worktrees in somebody's actual work.
fn must_be_disposable(at: &std::path::Path) {
    let tmp = std::env::temp_dir();
    let at = at.canonicalize().unwrap_or_else(|_| at.to_path_buf());
    let tmp = tmp.canonicalize().unwrap_or(tmp);
    assert!(
        at.starts_with(&tmp),
        "a test made a worktree at {} -- outside {}, which means it was pointed \
         at a repository that is not the fixture",
        at.display(),
        tmp.display()
    );
}

/// Run git in a fixture, and only there.
///
/// The environment is cleared because `GIT_DIR` beats `-C`. A suite run from a
/// git alias -- which `git work ship` is -- inherits one, and every fixture
/// here would quietly become a branch in whatever repository that pointed at.
fn git_at(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut cmd = std::process::Command::new("git");
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
    ] {
        cmd.env_remove(var);
    }
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git")
}

fn a_repo(name: &str) -> std::path::PathBuf {
    let dir = config_home().join(format!("repo-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("repo dir");
    git_at(&dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), b"one\n").expect("a file");
    git_at(&dir, &["add", "-A"]);
    git_at(&dir, &["commit", "-qm", "one"]);
    dir
}

/// Give a repository an upstream it has drifted from: two commits ahead of it
/// and one behind.
///
/// The upstream is a local branch. `@{upstream}` does not care which kind it
/// is -- it reads whatever the branch was configured against -- and a real
/// remote would mean a second directory and a fetch for no extra coverage.
fn diverged(dir: &std::path::Path) {
    git_at(dir, &["checkout", "-q", "-b", "base"]);
    git_at(dir, &["commit", "-q", "--allow-empty", "-m", "theirs"]);
    git_at(dir, &["checkout", "-q", "main"]);
    git_at(dir, &["commit", "-q", "--allow-empty", "-m", "mine one"]);
    git_at(dir, &["commit", "-q", "--allow-empty", "-m", "mine two"]);
    let out = git_at(dir, &["branch", "--set-upstream-to=base", "main"]);
    assert!(out.status.success(), "no upstream to drift from");
}

#[test]
fn a_worktree_and_somewhere_to_work_in_it_are_one_action() {
    // Doing it by hand is: leave dirk, `git worktree add`, come back, open the
    // project. The reason parallel agents on one repository are practical is
    // that this is the thing you do to start each of them.
    let repo = a_repo("wt");
    let session = unique("wt");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    let (ok, out) = ask(&session, &["worktree", "add", "feat/parallel"]);
    assert!(ok, "worktree add failed: {out}");

    let at = repo.parent().expect("a parent").join(format!(
        "{}-feat-parallel",
        repo.file_name().unwrap().to_string_lossy()
    ));
    must_be_disposable(&at);
    assert!(at.is_dir(), "no worktree at {}: {out}", at.display());

    // And a space open in it, on the branch. Polled, because what git says
    // about a checkout arrives on a thread of its own and a space that was made
    // a moment ago has not been asked about yet.
    let deadline = Instant::now() + START;
    let mut list = String::new();
    while Instant::now() < deadline {
        let (ok, out) = ask(&session, &["workspace", "list"]);
        list = out;
        if ok && list.contains("feat/parallel") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        list.contains("feat/parallel"),
        "no space opened on the new branch: {list}"
    );
    // Under the repository it belongs to, rather than as a project of its own
    // wearing a directory name nobody chose.
    assert_eq!(
        list.matches("\"project\"").count(),
        list.matches(&format!(
            "\"project\": \"{}\"",
            repo.file_name().unwrap().to_string_lossy()
        ))
        .count(),
        "the worktree opened as a separate project: {list}"
    );

    // Listing does not require leaving dirk, and says which one you are in.
    let (ok, trees) = ask(&session, &["worktree", "list"]);
    assert!(ok, "worktree list failed: {trees}");
    assert!(
        trees.contains("feat/parallel"),
        "the new one is missing: {trees}"
    );
    assert!(
        trees.contains("\"main\": true"),
        "nothing said which is the repository"
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&at);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn removing_a_worktree_refuses_while_it_holds_uncommitted_work() {
    // git refuses that by default, and the refusal is passed along rather than
    // decided for you: the case for removing it anyway is one only you can make.
    let repo = a_repo("wtrm");
    let session = unique("wtrm");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    let (ok, out) = ask(&session, &["worktree", "add", "scratch"]);
    assert!(ok, "worktree add failed: {out}");
    let at = repo.parent().expect("a parent").join(format!(
        "{}-scratch",
        repo.file_name().unwrap().to_string_lossy()
    ));
    must_be_disposable(&at);
    std::fs::write(at.join("a.txt"), b"changed\n").expect("dirty it");

    let (ok, why) = ask(&session, &["worktree", "remove", "scratch"]);
    assert!(!ok, "it removed work nobody had committed: {why}");
    assert!(at.is_dir(), "it went anyway");

    let (ok, out) = ask(&session, &["worktree", "remove", "scratch", "--force"]);
    assert!(ok, "forcing did not work: {out}");
    assert!(!at.is_dir(), "it is still there: {out}");

    // And the repository itself is not a worktree of itself.
    let (ok, why) = ask(&session, &["worktree", "remove", "main"]);
    assert!(!ok, "it offered to remove the repository: {why}");

    drop(client);
    let _ = std::fs::remove_dir_all(&repo);
}

// ── Tabs ────────────────────────────────────────────────────────────────

#[test]
fn a_space_holds_more_than_one_arrangement() {
    // An editor and a test runner are one task and two screens. The space keeps
    // its name and its agent's state, because those are about the task.
    let session = unique("tabs");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["tab", "list"]);
    assert!(ok, "tab list failed: {list}");
    assert_eq!(
        list.matches("\"id\"").count(),
        1,
        "a space starts with one tab"
    );

    let (ok, out) = ask(&session, &["tab", "new"]);
    assert!(ok, "tab new failed: {out}");
    let (ok, list) = ask(&session, &["tab", "list"]);
    assert!(ok, "tab list failed");
    assert_eq!(list.matches("\"id\"").count(), 2, "the second tab: {list}");

    // Still one space, because a tab is not one.
    let (ok, spaces) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    assert_eq!(
        spaces.matches("\"id\"").count(),
        1,
        "a new tab made a new space: {spaces}"
    );

    // Named by hand, and the name is what it is called from then on.
    let id = first_field(&list, "id");
    let (ok, _) = ask(&session, &["tab", "rename", &id, "tests"]);
    assert!(ok, "tab rename failed");
    let (ok, list) = ask(&session, &["tab", "list"]);
    assert!(ok, "tab list failed");
    assert!(list.contains("tests"), "the name did not stick: {list}");

    drop(client);
}

#[test]
fn the_last_tab_is_the_space_and_is_not_closed_on_its_own() {
    // Closing it is closing the space, which `x` on the last pane already
    // means. Two ways to do one thing, one of which leaves a row that draws
    // nothing, is the version worth refusing.
    let session = unique("lasttab");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["tab", "list"]);
    assert!(ok, "tab list failed");
    let id = first_field(&list, "id");
    let (ok, why) = ask(&session, &["tab", "close", &id]);
    assert!(!ok, "it closed the only tab: {why}");
    assert!(why.contains("last tab"), "said the wrong thing: {why}");

    drop(client);
}

#[test]
fn the_nav_draws_the_tabs_a_space_actually_has() {
    // The nav was already drawing a level here with nothing behind it.
    let session = unique("navtabs");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, _) = ask(&session, &["tab", "new"]);
    assert!(ok, "tab new failed");
    let (ok, list) = ask(&session, &["tab", "list"]);
    assert!(ok, "tab list failed");
    let id = first_field(&list, "id");
    let (ok, _) = ask(&session, &["tab", "rename", &id, "zzEDITOR"]);
    assert!(ok, "tab rename failed");

    // Expand the space, which is what the disclosure mark on its row does.
    // Found by its number rather than by the project's name: the name is the
    // directory this checkout happens to be in, which is not the same in a
    // worktree as it is in the repository.
    let rows = client.rows();
    // Below the spaces heading: the boards above it are numbered too, and
    // clicking one of those opens a board instead.
    let from = rows
        .iter()
        .position(|line| line.trim_start().starts_with("spaces"))
        .expect("the spaces heading");
    let (row, col) = rows
        .iter()
        .enumerate()
        .skip(from)
        .find_map(|(r, line)| {
            let at = line.find("1 ")?;
            line[..at].trim().is_empty().then_some((r, at))
        })
        .expect("the workspace row");
    // Once: the row is already focused, so the click is the disclosure.
    let mut client = client;
    client.click(col as u16, row as u16);

    assert!(
        client.wait_for("zzEDITOR", START),
        "the nav did not draw the tabs\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_pane_is_as_wide_as_the_narrowest_screen_showing_it() {
    // What tmux does, and the only answer that is not a lie to somebody: drawn
    // wider than the smallest client can show, it would be cut off there.
    let session = unique("narrow");

    let wide = Client::attach_sized(&session, 120, 30);
    assert!(wide.wait_for(READY, START), "never started");

    // Wide to start with, which is what the narrow client has to change.
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed: {list}");
    let cols = |json: &str| {
        let at = json.find("\"cols\"").expect("a width in the answer");
        json[at + 7..]
            .trim_start_matches([':', ' '])
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or("0")
            .parse::<u16>()
            .unwrap_or(0)
    };
    let was = cols(&list);
    assert!(was > 60, "the wide client's pane is only {was} columns");

    // A narrow one arrives, and the pane is now as wide as it can manage.
    let narrow = Client::attach_sized(&session, 60, 20);
    assert!(
        narrow.wait_for(READY, START),
        "the narrow client never drew"
    );

    let narrowed = |_: &()| {
        let (ok, panes) = ask(&session, &["pane", "list"]);
        ok && cols(&panes) < was
    };
    let deadline = Instant::now() + START;
    while Instant::now() < deadline && !narrowed(&()) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let now = cols(&panes);
    assert!(
        now < was,
        "the pane stayed {was} wide with a {now}-column client watching: {panes}"
    );

    drop(narrow);
    drop(wide);
}

#[test]
fn a_worktree_joins_the_project_it_is_a_worktree_of() {
    // A project is keyed by its repository, not by its path. Keyed by path,
    // every worktree opened as its own top-level project wearing whatever the
    // directory happened to be called.
    let repo = a_repo("group");
    let session = unique("group");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    let (ok, out) = ask(&session, &["worktree", "add", "side"]);
    assert!(ok, "worktree add failed: {out}");
    let at = repo.parent().unwrap().join(format!(
        "{}-side",
        repo.file_name().unwrap().to_string_lossy()
    ));

    must_be_disposable(&at);
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let name = repo.file_name().unwrap().to_string_lossy().into_owned();
    // Two spaces, one project, two directories.
    assert_eq!(
        list.matches("\"id\"").count(),
        2,
        "expected two spaces: {list}"
    );
    assert_eq!(
        list.matches(&format!("\"project\": \"{name}\"")).count(),
        2,
        "the worktree opened as a project of its own: {list}"
    );
    assert!(
        list.contains(&at.to_string_lossy().into_owned()),
        "no space in the worktree's own directory: {list}"
    );

    // And removing the worktree leaves the repository open.
    let (ok, out) = ask(&session, &["worktree", "remove", "side", "--force"]);
    assert!(ok, "worktree remove failed: {out}");
    let (ok, after) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    assert_eq!(
        after.matches("\"id\"").count(),
        1,
        "removing a worktree closed the repository too: {after}"
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&at);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn each_checkout_reports_its_own_branch() {
    // Two worktrees of one repository are on two branches -- which is the
    // entire reason somebody made the second one. One answer for both would be
    // wrong for at least one of them.
    let repo = a_repo("branches");
    let session = unique("branches");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    let (ok, out) = ask(&session, &["worktree", "add", "other"]);
    assert!(ok, "worktree add failed: {out}");

    let deadline = Instant::now() + START;
    let mut list = String::new();
    while Instant::now() < deadline {
        let (ok, out) = ask(&session, &["workspace", "list"]);
        list = out;
        if ok && list.contains("\"branch\": \"other\"") && list.contains("\"branch\": \"main\"") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        list.contains("\"branch\": \"main\""),
        "the repository proper lost its branch: {list}"
    );
    assert!(
        list.contains("\"branch\": \"other\""),
        "the worktree reported the wrong branch: {list}"
    );

    drop(client);
    let at = repo.parent().unwrap().join(format!(
        "{}-other",
        repo.file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&at);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_git_environment_from_outside_does_not_redirect_dirk() {
    // `GIT_DIR` beats `git -C`, so a dirk started from anywhere that exports
    // one -- a git alias, a hook, `git rebase --exec` -- would read *that*
    // repository instead of the directory it was asked about, and every answer
    // would be confidently wrong.
    //
    // It put four branches and two worktrees into this repository before
    // anybody noticed, which is why the test names the failure rather than the
    // fix.
    let repo = a_repo("envguard");
    let session = unique("envguard");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
            // Pointed somewhere else entirely, the way a git alias would.
            cmd.env("GIT_DIR", "/definitely/not/this/one/.git");
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    let (ok, trees) = ask(&session, &["worktree", "list"]);
    assert!(ok, "worktree list failed: {trees}");
    assert!(
        trees.contains(&repo.to_string_lossy().into_owned()),
        "dirk read the repository the environment named, not the one it is in: {trees}"
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_space_leads_with_what_it_is_and_captions_it_with_what_it_is_doing() {
    // The intent is rewritten every time the agent revises what it says it is
    // up to. Leading with it makes the column you scan for somewhere to go the
    // one column that will not hold still.
    let repo = a_repo("leads");
    let session = unique("leads");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let id = first_field(&list, "id");
    let (ok, out) = ask(&session, &["workspace", "rename", &id, "zzINTENT"]);
    assert!(ok, "workspace rename failed: {out}");
    assert!(
        client.wait_for("zzINTENT", START),
        "the name never landed\n{}",
        client.drawn()
    );
    // The checkout is read on a timer. Until that first read lands the space
    // has no branch, and its label is correctly on the identity line -- which
    // is the other half of this behaviour and not the half being tested here.
    assert!(
        client.wait_until(START, |c| c.rows().iter().any(|r| r.contains("main"))),
        "the branch was never read\n{}",
        client.drawn()
    );

    let rows = client.rows();
    let at = rows
        .iter()
        .position(|line| line.contains("zzINTENT"))
        .expect("the intent line");
    assert!(at > 0, "the intent is the first row the nav drew");
    assert!(
        rows[at - 1].contains("main"),
        "the branch is not on the line above the intent it names\n{}",
        client.drawn()
    );
    assert!(
        !rows[at].contains("main"),
        "identity and intent are on one line\n{}",
        client.drawn()
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_space_with_no_branch_says_its_name_once() {
    // Outside a repository the label is the only name there is, so it goes on
    // the identity line -- and a caption underneath repeating it would be a
    // row that says the same thing twice.
    let session = unique("oneline");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let id = first_field(&list, "id");
    let (ok, out) = ask(&session, &["workspace", "rename", &id, "zzONLY"]);
    assert!(ok, "workspace rename failed: {out}");
    assert!(
        client.wait_for("zzONLY", START),
        "the name never landed\n{}",
        client.drawn()
    );

    let rows = client.rows();
    let at = rows
        .iter()
        .position(|line| line.contains("zzONLY"))
        .expect("the name");
    assert!(
        !rows.get(at + 1).is_some_and(|r| r.contains("zzONLY")),
        "the name is drawn twice, once as itself and once as its own caption\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_branch_says_how_far_it_has_gone_and_how_far_it_has_been_left() {
    // Twenty-two commits behind is the difference between work you can ship
    // and work that is about to conflict, and finding out meant leaving.
    let repo = a_repo("track");
    diverged(&repo);
    let session = unique("track");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    // The checkout is read on a timer and off the drawing thread, so the
    // answer arrives after the row it belongs to.
    let deadline = Instant::now() + START;
    let mut list = String::new();
    while Instant::now() < deadline {
        let (ok, out) = ask(&session, &["workspace", "list"]);
        list = out;
        if ok && list.contains("\"ahead\": 2") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        list.contains("\"ahead\": 2"),
        "two commits ahead of its upstream went unsaid: {list}"
    );
    assert!(
        list.contains("\"behind\": 1"),
        "one commit behind its upstream went unsaid: {list}"
    );

    // And on the row itself, beside the branch it is about.
    assert!(
        client.wait_for("↑2", START),
        "nothing on the row said how far ahead it was\n{}",
        client.drawn()
    );
    assert!(
        client.drawn().contains("↓1"),
        "nothing on the row said how far behind it was\n{}",
        client.drawn()
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_branch_with_nothing_to_compare_against_says_nothing_rather_than_zero() {
    // "0" for "there is no upstream" is the more misleading of the two: it
    // reads as up to date with something, and there is no something.
    let repo = a_repo("untracked");
    let session = unique("untracked");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    // Wait for the checkout to have been read at all, or this passes because
    // nothing has happened yet rather than because nothing is drawn.
    let deadline = Instant::now() + START;
    let mut list = String::new();
    while Instant::now() < deadline {
        let (ok, out) = ask(&session, &["workspace", "list"]);
        list = out;
        if ok && list.contains("\"branch\": \"main\"") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        list.contains("\"branch\": \"main\""),
        "the branch was never read, so this proves nothing: {list}"
    );
    assert!(
        list.contains("\"ahead\": null") && list.contains("\"behind\": null"),
        "a branch with no upstream was given numbers: {list}"
    );
    let drawn = client.drawn();
    assert!(
        !drawn.contains('↑') && !drawn.contains('↓'),
        "counts drawn for a branch with nothing to compare against\n{drawn}"
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_worktrees_space_hangs_off_the_checkout_the_others_hang_off() {
    // A worktree is not a peer of the repository proper -- it is the same work
    // on another branch, which is the whole reason somebody made one.
    let repo = a_repo("hang");
    let session = unique("hang");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let repo = repo.clone();
        move |cmd| {
            cmd.cwd(&repo);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    let (ok, out) = ask(&session, &["worktree", "add", "side"]);
    assert!(ok, "worktree add failed: {out}");
    let at = repo.parent().unwrap().join(format!(
        "{}-side",
        repo.file_name().unwrap().to_string_lossy()
    ));
    must_be_disposable(&at);
    assert!(
        client.wait_for("side", START),
        "the worktree never reached the column\n{}",
        client.drawn()
    );

    // The repository proper's space is flush and the worktree's hangs under
    // it, one level in, off a connector. Measured at the branch rather than at
    // the leading whitespace: the connector sits outside the indent, which is
    // the whole of what a connector is for.
    let rows = client.rows();
    let from = rows
        .iter()
        .position(|line| line.trim_start().starts_with("spaces"))
        .expect("the spaces heading");
    // Columns, not byte offsets: a connector is three bytes and one column,
    // and counting bytes would make the indent look bigger than it is.
    let column = |line: &str, want: &str| line.find(want).map(|at| line[..at].chars().count());
    let below = rows.iter().skip(from).collect::<Vec<_>>();
    let flush = below
        .iter()
        .find_map(|line| column(line, "main"))
        .expect("no space on the repository proper's branch");
    let hung = below
        .iter()
        .find(|line| line.contains('└') || line.contains('├'))
        .expect("no connector: nothing was drawn as hanging");
    let hung_at = column(hung, "side").expect("the worktree's branch");
    assert!(
        hung_at > flush,
        "the worktree was drawn as a peer of the repository proper: \
         `side` at {hung_at}, `main` at {flush}\n{}",
        client.drawn()
    );

    // And everything under it moves with it. A caption indented differently
    // from the line it is a caption of reads as belonging to something else.
    let caption = |branch: &str| {
        let row = below.iter().position(|line| line.contains(branch))?;
        below.get(row + 1).and_then(|line| {
            let text = line.trim_start();
            (!text.is_empty()).then(|| line.len() - text.len())
        })
    };
    assert_eq!(
        caption("side").zip(caption("main")).map(|(a, b)| a - b),
        Some(hung_at - flush),
        "the caption did not move in with the row it captions\n{}",
        client.drawn()
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&at);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_folded_row_says_whether_opening_it_would_show_you_anything() {
    // A row with children said so only by having them, and only once opened,
    // so the disclosure was a thing you tried rather than a thing you read.
    let session = unique("disclose");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    // One space with one pane: nothing to open, so nothing to say about it.
    // A dimmed mark would still be a mark.
    let row_of = |c: &Client, n: &str| {
        let rows = c.rows();
        let from = rows
            .iter()
            .position(|line| line.trim_start().starts_with("spaces"))?;
        rows.iter()
            .enumerate()
            .skip(from)
            .find(|(_, line)| {
                line.find(n)
                    .is_some_and(|at| line[..at].trim().is_empty() && line[at..].starts_with(n))
            })
            .map(|(r, line)| (r, line.clone()))
    };
    let (_, alone) = row_of(&client, "1 ").expect("the space's row");
    assert!(
        !alone.contains('▸') && !alone.contains('▾'),
        "a space with nothing under it carried a disclosure: |{alone}|"
    );

    // Give it a second pane, and the mark appears at the right edge.
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");
    let (ok, out) = ask(&session, &["pane", "split", &pane, "cols"]);
    assert!(ok, "split failed: {out}");
    assert!(
        client.wait_until(START, |c| row_of(c, "1 ")
            .is_some_and(|(_, line)| line.contains('▸') || line.contains('▾'))),
        "a space with two panes said nothing about them\n{}",
        client.drawn()
    );
    let (row, line) = row_of(&client, "1 ").expect("the space's row");
    let mark = line
        .char_indices()
        .filter(|(_, c)| *c == '▸' || *c == '▾')
        .map(|(at, _)| line[..at].chars().count())
        .next_back()
        .expect("the disclosure");
    assert!(
        mark > line.chars().count() / 2,
        "the disclosure is not at the right edge: column {mark} of {}",
        line.chars().count()
    );

    // And clicking it opens the row, rather than being a click you have to
    // guess at.
    let mut client = client;
    client.click(mark as u16, row as u16);
    assert!(
        client.wait_until(START, |c| row_of(c, "1 ")
            .is_some_and(|(_, line)| line.contains('▾'))),
        "clicking the mark did not open the row\n{}",
        client.drawn()
    );
    client.click(mark as u16, row as u16);
    assert!(
        client.wait_until(START, |c| row_of(c, "1 ")
            .is_some_and(|(_, line)| line.contains('▸'))),
        "clicking it again did not close the row\n{}",
        client.drawn()
    );

    drop(client);
}

#[test]
fn a_chosen_glyph_set_reaches_the_column() {
    // A set that resolves correctly and is never asked for is not a set
    // anybody has. This is the only thing the unit tests cannot say: that the
    // name in the file is the name the nav draws with.
    let home = config_home().join("round");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("dirk")).expect("config dir");
    std::fs::write(
        home.join("dirk").join("config.toml"),
        "[nav]\nglyphs = \"round\"\n",
    )
    .expect("config");

    let session = unique("round");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let home = home.clone();
        move |cmd| {
            cmd.env("XDG_CONFIG_HOME", &home);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    // A state has to exist before a state mark can be drawn: a shell at a
    // prompt draws nothing in either set, which is correct and proves nothing.
    let (ok, out) = ask(&session, &["agent", "state", "blocked"]);
    assert!(ok, "the report was refused: {out}");
    assert!(
        client.wait_for("●", START),
        "the round set was configured and the nav drew something else\n{}",
        client.drawn()
    );
    assert!(
        !client.drawn().contains('!'),
        "the unicode mark for blocked was drawn as well\n{}",
        client.drawn()
    );

    drop(client);
    let _ = std::fs::remove_dir_all(&home);
}

/// A configuration directory holding one file, for a test that needs the
/// session to have been started with a setting rather than without it.
fn configured(name: &str, body: &str) -> std::path::PathBuf {
    let dir = config_home().join(format!("cfg-{name}"));
    std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
    std::fs::write(dir.join("dirk").join("config.toml"), body).expect("config");
    dir
}

/// The cols each pane reports, in the order `pane list` gave them.
fn widths(json: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(at) = rest.find("\"cols\"") {
        rest = &rest[at + 6..];
        let digits: String = rest
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() {
            break;
        }
        out.push(digits.parse().unwrap_or(0));
    }
    out
}

#[test]
fn a_pane_made_with_nobody_watching_gets_the_size_that_was_configured() {
    // A pane is sized from the client watching it, and a session driven from a
    // script has no client. What it used to get was the size of whichever
    // terminal had most recently been attached -- so a caller that created a
    // workspace, read a pane, and parsed the output was parsing it at the wrap
    // points of a screen that had gone home.
    let session = unique("headless");
    // Deliberately wider than any client this suite attaches, so the number can
    // only have come from the configuration.
    let dir = configured(
        "headless",
        "[server]\nheadless_cols = 200\nheadless_rows = 60\n",
    );
    let mut client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    // While somebody is watching, the client wins: that is the screen the pane
    // is going to be drawn on.
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed: {list}");
    let watched = widths(&list);
    assert!(!watched.is_empty(), "no panes: {list}");
    assert!(
        watched.iter().all(|w| *w < COLS as u64),
        "an attached client's pane was not sized from its terminal: {watched:?}"
    );

    // Leave. The panes that exist keep the size they were drawn at.
    client.kill();
    let gone = Instant::now() + START;
    while Instant::now() < gone {
        let (_, info) = ask(&session, &["session", "info"]);
        if info.contains("\"attached\": false") {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed after detaching: {list}");
    assert_eq!(
        widths(&list),
        watched,
        "detaching resized panes that already had a size"
    );

    // The next one made belongs to a session nobody is looking at.
    let (ok, made) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed: {made}");
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed: {list}");
    let after = widths(&list);
    assert!(
        after.iter().any(|w| *w > COLS as u64),
        "a pane made with nobody watching was sized from a terminal that had left: {after:?}"
    );

    end(&session);
}

/// Ask something without waiting for the answer here.
///
/// A wait does not return until the session says so, and the whole point is to
/// be doing something else meanwhile — which is also what the caller in the
/// field is doing.
fn ask_later(session: &str, args: &[&str]) -> std::thread::JoinHandle<(bool, String)> {
    let session = session.to_string();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    std::thread::spawn(move || {
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        ask(&session, &borrowed)
    })
}

#[test]
fn waiting_for_an_agent_ends_when_the_agent_does_the_thing() {
    // The alternative is the loop everybody writes: ask every few hundred
    // milliseconds, pick a different interval from everybody else, and miss a
    // state that was entered and left between two of them.
    let session = unique("wait");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    // Somewhere that is not the workspace on screen, and an agent in it.
    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let all = ids(&list);
    let ws = all.last().expect("a second workspace").clone();
    let (ok, out) = ask(&session, &["agent", "state", "working", &ws]);
    assert!(ok, "the report was refused: {out}");

    // Held, not polled: nothing has happened yet, so nothing comes back.
    let waiting = ask_later(
        &session,
        &[
            "agent",
            "wait",
            &ws,
            "--until",
            "blocked",
            "--timeout",
            "30000",
        ],
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !waiting.is_finished(),
        "the wait returned before anything happened"
    );

    // A wait is about a pane, not about where the nav happens to be pointing.
    // Renaming and refocusing both happen to a workspace an agent is working
    // in, continuously, and neither is the thing being waited for.
    let (ok, _) = ask(
        &session,
        &["workspace", "rename", &ws, "renamed", "midwait"],
    );
    assert!(ok, "workspace rename failed");
    let (ok, _) = ask(&session, &["workspace", "focus", &ws]);
    assert!(ok, "workspace focus failed");
    assert!(
        !waiting.is_finished(),
        "renaming ended a wait that was about an agent"
    );

    let (ok, out) = ask(&session, &["agent", "state", "blocked", &ws]);
    assert!(ok, "the report was refused: {out}");

    let (ok, said) = waiting.join().expect("the wait thread");
    assert!(ok, "the wait failed: {said}");
    assert!(
        said.contains("\"state\": \"blocked\""),
        "the wait did not answer with the agent it was about: {said}"
    );

    // Asked and already true is answered now. A caller waiting for `blocked` on
    // an agent that is already blocked should not wait at all.
    let at = Instant::now();
    let (ok, said) = ask(&session, &["agent", "wait", &ws, "--until", "blocked"]);
    assert!(ok, "a wait for a state already reached failed: {said}");
    assert!(
        at.elapsed() < Duration::from_secs(5),
        "a wait for a state already reached did not return promptly"
    );

    drop(client);
}

#[test]
fn a_wait_that_cannot_be_answered_says_so_rather_than_waiting() {
    let session = unique("waitbad");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = first_id(&list);
    let (ok, out) = ask(&session, &["agent", "state", "working", &ws]);
    assert!(ok, "the report was refused: {out}");

    // A state that does not exist is refused at the door. Waiting out a
    // timeout for something that can never happen is the worst way to learn
    // that a word was misspelled.
    let at = Instant::now();
    let (ok, said) = ask(&session, &["agent", "wait", &ws, "--until", "finished"]);
    assert!(!ok, "a state that does not exist was accepted: {said}");
    assert!(
        at.elapsed() < Duration::from_secs(5),
        "a misspelled state was waited out rather than refused"
    );

    // A target that does not exist, likewise.
    let (ok, _) = ask(&session, &["agent", "wait", "w9999"]);
    assert!(
        !ok,
        "a wait on a workspace that does not exist was accepted"
    );

    // And patience that runs out says timeout, in the milliseconds it was given
    // rather than on whatever tick comes next.
    let at = Instant::now();
    let (ok, said) = ask(
        &session,
        &["agent", "wait", &ws, "--until", "done", "--timeout", "700"],
    );
    assert!(!ok, "a wait that should have timed out succeeded: {said}");
    assert!(
        said.contains("timeout"),
        "the failure did not say timeout: {said}"
    );
    assert!(
        at.elapsed() < Duration::from_secs(5),
        "the timeout took far longer than it was given"
    );

    drop(client);
}

#[test]
fn an_agent_that_goes_away_ends_the_wait_saying_so() {
    // Distinct from a timeout, and it has to be: a caller that timed out might
    // reasonably wait again, and one whose agent has gone should not.
    let session = unique("waitgone");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = ids(&list).last().expect("a second workspace").clone();
    let (ok, out) = ask(&session, &["agent", "state", "working", &ws]);
    assert!(ok, "the report was refused: {out}");

    let waiting = ask_later(&session, &["agent", "wait", &ws, "--until", "done"]);
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !waiting.is_finished(),
        "the wait returned before anything happened"
    );

    let (ok, _) = ask(&session, &["workspace", "close", &ws]);
    assert!(ok, "workspace close failed");

    let (ok, said) = waiting.join().expect("the wait thread");
    assert!(!ok, "a wait on an agent that went away succeeded: {said}");
    assert!(
        !said.contains("timeout"),
        "an agent going away was reported as a timeout: {said}"
    );

    drop(client);
}

#[test]
fn a_command_text_and_a_keystroke_are_three_different_things() {
    // One verb meant every caller wrote the submitting return itself, and got
    // the ordering wrong against anything slow to read. It also meant there was
    // no way to say Escape at all: a key is not characters, which is the whole
    // difference between a key and text.
    let session = unique("input");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let id = first_id(&panes);

    // Text alone does not submit. If it did, this would run before the rest of
    // the line arrived, which is the failure the split exists to prevent.
    let (ok, out) = ask(&session, &["pane", "send-text", &id, "printf zzHALF"]);
    assert!(ok, "send-text failed: {out}");
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !ask(&session, &["pane", "read", &id, "20"])
            .1
            .contains("zzHALFzz"),
        "send-text submitted the line it was given"
    );

    // Enter, as a key rather than as a character, finishes it.
    let (ok, out) = ask(&session, &["pane", "send-keys", &id, "enter"]);
    assert!(ok, "send-keys failed: {out}");
    assert!(
        appears_in_pane(&session, &id, "zzHALF", START),
        "the line never ran"
    );

    // And a command is both, in one write.
    let (ok, out) = ask(&session, &["pane", "run", &id, "printf zzWHOLE"]);
    assert!(ok, "pane run failed: {out}");
    assert!(
        appears_in_pane(&session, &id, "zzWHOLE", START),
        "pane run did not submit its command"
    );

    // A name dirk does not know is refused rather than typed. The bad outcome
    // is not an error; it is those six letters arriving in the agent.
    let (ok, out) = ask(&session, &["pane", "send-keys", &id, "escpae"]);
    assert!(!ok, "a key that does not exist was accepted: {out}");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !ask(&session, &["pane", "read", &id, "20"])
            .1
            .contains("escpae"),
        "a rejected key name was typed into the pane anyway"
    );

    drop(client);
}

/// Wait for text to come back out of a pane, the way a caller would.
fn appears_in_pane(session: &str, pane: &str, needle: &str, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if ask(session, &["pane", "read", pane, "40"])
            .1
            .contains(needle)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}
