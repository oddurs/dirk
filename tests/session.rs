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

//! `unwrap` here is an assertion. `clippy.toml` says so for `#[cfg(test)]`
//! modules; a suite is its own crate and has to say it itself.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod fixture;

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
    let _ = std::fs::create_dir_all(dir.join("dirk"));
    // Non-login shells, for the same reason the suite sets `SHELL` and clears
    // the `GIT_*` variables: these tests assert on what is drawn, and a login
    // shell brings /etc/profile, /etc/bashrc and somebody's prompt into it. A
    // prompt that publishes its directory as a window title is a workspace
    // label, and a label of a different length moves every column after it.
    //
    // The test that is *about* login shells writes its own configuration.
    let _ = std::fs::write(
        dir.join("dirk").join("config.toml"),
        "[terminal]\nshell_mode = \"non_login\"\n",
    );
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

use fixture::git as git_at;

fn a_repo(name: &str) -> std::path::PathBuf {
    let dir = config_home().join(format!("repo-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    fixture::repo(&dir, "main")
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
    // Asked of the session rather than watched for on the screen. A pane's own
    // prompt carries its directory — `repo-hang-side` — so waiting for "side"
    // to appear anywhere was satisfied by the shell before git had answered,
    // and the branch had not reached the column yet.
    let branched = Instant::now() + START;
    while Instant::now() < branched {
        if ask(&session, &["workspace", "list"])
            .1
            .contains("\"branch\": \"side\"")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
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
    let hung_at =
        column(hung, "side").unwrap_or_else(|| panic!("the worktree's branch\n{}", client.drawn()));
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

#[test]
fn waiting_for_a_line_out_of_a_pane_that_holds_no_agent() {
    // Half of what runs in a pane is not an agent: a test watcher, a dev
    // server, a deploy. `agent wait` cannot help there and there was nothing
    // else, so the caller polled `pane read` on a timer.
    let session = unique("output");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let id = first_id(&panes);

    // Held: the line has not been printed yet.
    let waiting = ask_later(
        &session,
        &["pane", "wait-output", &id, "zzLANDED", "--timeout", "30000"],
    );
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !waiting.is_finished(),
        "the wait returned before the line was printed"
    );

    let (ok, out) = ask(&session, &["pane", "run", &id, "printf 'zzLANDED\\n'"]);
    assert!(ok, "pane run failed: {out}");

    let (ok, said) = waiting.join().expect("the wait thread");
    assert!(ok, "the wait failed: {said}");
    assert!(
        said.contains("zzLANDED"),
        "the wait did not answer with the line it matched: {said}"
    );

    // Text already on the screen matches at once. A caller that starts a
    // command and then waits for its output must not lose that race.
    let at = Instant::now();
    let (ok, said) = ask(&session, &["pane", "wait-output", &id, "zzLANDED"]);
    assert!(ok, "a wait for a line already printed failed: {said}");
    assert!(
        at.elapsed() < Duration::from_secs(5),
        "a wait for a line already printed did not return promptly"
    );

    // A pattern, matched one line at a time.
    let (ok, out) = ask(&session, &["pane", "run", &id, "printf 'zzOK 7 passed\\n'"]);
    assert!(ok, "pane run failed: {out}");
    let (ok, said) = ask(
        &session,
        &[
            "pane",
            "wait-output",
            &id,
            "^zzOK [0-9]+ passed$",
            "--regex",
            "--timeout",
            "30000",
        ],
    );
    assert!(ok, "the pattern never matched: {said}");

    // One that does not parse is refused rather than waited out.
    let (ok, said) = ask(
        &session,
        &["pane", "wait-output", &id, "(unclosed", "--regex"],
    );
    assert!(!ok, "a pattern that does not parse was accepted: {said}");

    // So is an option this command does not take. On a command that waits, a
    // misspelling is not a wrong answer -- it is no answer at all, until a
    // timeout that was probably misspelled too.
    let (ok, said) = ask(
        &session,
        &[
            "pane",
            "wait-output",
            &id,
            "zzNEVER",
            "--regexp",
            "--timeout",
            "700",
        ],
    );
    assert!(!ok, "an option that does not exist was accepted: {said}");
    assert!(
        said.contains("regexp"),
        "the failure did not name the option it did not understand: {said}"
    );

    // And patience that runs out says timeout.
    let (ok, said) = ask(
        &session,
        &["pane", "wait-output", &id, "zzNEVER", "--timeout", "700"],
    );
    assert!(!ok, "a wait that should have timed out succeeded: {said}");
    assert!(
        said.contains("timeout"),
        "the failure did not say timeout: {said}"
    );

    drop(client);
}

#[test]
fn a_prompt_is_written_once_and_refused_when_the_agent_is_asking() {
    // dirk could start an agent and could recognise one, and could not talk to
    // one. That is the whole of what this closes.
    let session = unique("prompt");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = ids(&list).last().expect("a second workspace").clone();
    let (ok, out) = ask(&session, &["agent", "state", "idle", &ws]);
    assert!(ok, "the report was refused: {out}");

    // The shell in that pane is a stand-in for an agent's interface: what is
    // being tested is that the text and its return arrive together, and a shell
    // shows that by running the line.
    let (ok, said) = ask(
        &session,
        &["agent", "prompt", &ws, "printf 'zzPROMPTED\\n'"],
    );
    assert!(ok, "the prompt was refused: {said}");
    assert!(
        said.contains("\"agent\""),
        "the answer did not carry the agent it was about: {said}"
    );
    let (ok, panes) = ask(&session, &["pane", "list", &ws]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    assert!(
        appears_in_pane(&session, &pane, "zzPROMPTED", START),
        "the prompt was not submitted"
    );

    // An agent sitting on a question is not one to type a prompt at: the dialog
    // wants an answer, and a prompt would be read as one.
    let (ok, out) = ask(&session, &["agent", "state", "blocked", &ws]);
    assert!(ok, "the report was refused: {out}");
    let (ok, said) = ask(&session, &["agent", "prompt", &ws, "printf 'zzNOTSENT\\n'"]);
    assert!(!ok, "a blocked agent was typed at: {said}");
    assert!(
        said.contains("blocked"),
        "the refusal did not say why: {said}"
    );
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !ask(&session, &["pane", "read", &pane, "40"])
            .1
            .contains("zzNOTSENT"),
        "a refused prompt was written to the pane anyway"
    );

    drop(client);
}

#[test]
fn waiting_on_a_prompt_waits_for_it_to_have_started_something() {
    // Without this the wait is satisfied by the state the agent was already in:
    // one that was idle when you prompted it is still idle an instant later,
    // for the same reason as before.
    let session = unique("promptwait");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = ids(&list).last().expect("a second workspace").clone();
    let (ok, out) = ask(&session, &["agent", "state", "idle", &ws]);
    assert!(ok, "the report was refused: {out}");

    let waiting = ask_later(
        &session,
        &[
            "agent",
            "prompt",
            &ws,
            "true",
            "--wait",
            "--timeout",
            "30000",
        ],
    );
    // Idle throughout would once have ended this immediately.
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        !waiting.is_finished(),
        "the wait was satisfied by the state the agent was already in"
    );

    let (ok, out) = ask(&session, &["agent", "state", "working", &ws]);
    assert!(ok, "the report was refused: {out}");
    std::thread::sleep(Duration::from_millis(300));
    let (ok, out) = ask(&session, &["agent", "state", "done", &ws]);
    assert!(ok, "the report was refused: {out}");

    let (ok, said) = waiting.join().expect("the wait thread");
    assert!(ok, "the wait failed: {said}");
    assert!(
        said.contains("\"state\": \"done\""),
        "the wait did not answer with the settled agent: {said}"
    );

    drop(client);
}

#[test]
fn a_prompt_that_starts_nothing_says_so_rather_than_waiting_it_out() {
    let session = unique("stalled");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = ids(&list).last().expect("a second workspace").clone();
    let (ok, out) = ask(&session, &["agent", "state", "idle", &ws]);
    assert!(ok, "the report was refused: {out}");

    // Nothing will ever report working here. The wait should give up on the
    // prompt having started something, not sit until the timeout.
    let at = Instant::now();
    let (ok, said) = ask(
        &session,
        &[
            "agent",
            "prompt",
            &ws,
            "true",
            "--wait",
            "--timeout",
            "60000",
        ],
    );
    assert!(
        !ok,
        "a prompt that started nothing was called a success: {said}"
    );
    assert!(
        at.elapsed() < Duration::from_secs(30),
        "it waited out the caller's timeout rather than its own"
    );
    assert!(
        said.contains("sent"),
        "the failure did not say the prompt had been sent: {said}"
    );

    drop(client);
}

#[test]
fn the_surface_describes_itself_without_a_session_to_ask() {
    // A caller generating a client wants this before there is a session to ask,
    // which is most of why it is not just another command.
    let session = unique("schema");
    let (ok, said) = ask(&session, &["api", "schema"]);
    assert!(ok, "api schema needed a session: {said}");
    assert!(
        said.contains("\"api.schema\""),
        "the schema omits itself: {said}"
    );
    assert!(
        said.contains("\"agent.wait\""),
        "the schema omits a command: {said}"
    );
    assert!(
        said.contains("\"answer\""),
        "the schema does not say what an answer carries: {said}"
    );
    let socket = std::env::temp_dir()
        .join(format!("dirk-{}", unsafe { libc::getuid() }))
        .join(format!("{session}.sock"));
    assert!(!socket.exists(), "asking for the schema started a session");

    // Twice, byte for byte. A schema that reordered itself between runs would
    // make every diff of the checked-in copy unreadable.
    let (_, again) = ask(&session, &["api", "schema"]);
    assert_eq!(said, again, "the schema is not stable between runs");

    // And a running session answers the same thing down the socket, so a caller
    // already holding one does not have to shell out to ask what it may say.
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    let (ok, over_socket) = ask(&session, &["api", "schema"]);
    assert!(ok, "the session refused api schema: {over_socket}");
    drop(client);
}

#[test]
fn the_completion_scripts_are_scripts_the_shells_accept() {
    // Generated text that does not parse is worse than none: it is sourced from
    // a startup file, and a syntax error there is a shell that complains on
    // every new terminal.
    let session = unique("completion");
    for shell in ["bash", "zsh", "fish"] {
        let (ok, text) = ask(&session, &["completion", shell]);
        assert!(ok, "no {shell} completions: {text}");
        assert!(
            text.contains("wait-output"),
            "the {shell} script is missing a verb the table has"
        );

        // Skipped rather than failed when the shell is not installed: a suite
        // that needs three shells present is one that is red on most machines.
        if std::process::Command::new(shell)
            .arg("--version")
            .output()
            .is_err()
        {
            continue;
        }
        let file = std::env::temp_dir().join(format!("dirk-completion-{shell}"));
        std::fs::write(&file, &text).expect("write the script");
        let out = std::process::Command::new(shell)
            .arg("-n")
            .arg(&file)
            .output()
            .expect("run the shell");
        assert!(
            out.status.success(),
            "{shell} rejected its own completion script:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let _ = std::fs::remove_file(&file);
    }

    // A shell there is no script for is said so, rather than handed an empty
    // file that would silently complete nothing.
    let (ok, said) = ask(&session, &["completion", "nushell"]);
    assert!(!ok, "a shell with no script was accepted: {said}");
}

#[test]
fn ids_complete_from_the_session_that_has_them() {
    // The half that makes this worth having: `w7:p12` is not a thing anybody
    // remembers, and it is the argument almost every command wants. The scripts
    // get it by asking `dirk pane list`, so what has to hold is that the answer
    // is still shaped the way they read it.
    let session = unique("completeids");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let expected = first_id(&panes);
    let plucked: Vec<String> = panes
        .lines()
        .filter_map(|line| {
            let at = line.find("\"id\"")?;
            let rest = &line[at + 4..];
            let open = rest.find('"')?;
            let rest = &rest[open + 1..];
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect();
    assert!(
        plucked.contains(&expected),
        "the shape the completions pluck ids out of has changed: {panes}"
    );

    drop(client);
}

#[test]
fn a_script_can_cause_an_interruption_and_gets_the_same_rules() {
    // dirk knows how to make a noise on the machine somebody is sitting at, and
    // only its own state machine could ask for one. A build, a deploy or a cron
    // job is owed exactly what an agent is owed — including the rules that
    // decide whether this is a feature or something muted within a week.
    let session = unique("notify");
    let (dir, rang) = noisy("script");
    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    // Somewhere that is not on screen.
    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let all = ids(&list);
    let ws = all.last().expect("a second workspace").clone();
    let (ok, _) = ask(&session, &["workspace", "focus", &all[0]]);
    assert!(ok, "workspace focus failed");

    let (ok, said) = ask(
        &session,
        &["session", "notify", &ws, "the deploy failed", "--blocked"],
    );
    assert!(ok, "the notification was refused: {said}");
    assert!(
        said.contains("\"notified\": true"),
        "it declined to notify: {said}"
    );
    assert!(
        appears(&rang, START),
        "nothing rang on the machine with the speakers\n{}",
        client.drawn()
    );

    // The floor applies. An agent that blocks, unblocks and blocks again inside
    // a minute is one interruption, and so is a script in a loop.
    let (ok, said) = ask(
        &session,
        &[
            "session",
            "notify",
            &ws,
            "the deploy failed again",
            "--blocked",
        ],
    );
    assert!(
        ok,
        "the second notification errored rather than declining: {said}"
    );
    assert!(
        said.contains("\"notified\": false"),
        "the floor did not apply to a script: {said}"
    );

    // And nothing about the workspace you are looking at, which is the rule
    // that decides whether this gets muted.
    let (ok, said) = ask(
        &session,
        &["session", "notify", &all[0], "you can see this"],
    );
    assert!(ok, "the notification errored rather than declining: {said}");
    assert!(
        said.contains("looking at it"),
        "it interrupted about the workspace on screen: {said}"
    );

    // A target that does not exist is refused rather than silently dropped.
    let (ok, _) = ask(&session, &["session", "notify", "w9999", "nowhere"]);
    assert!(
        !ok,
        "a notification about a workspace that does not exist was accepted"
    );
    // As is one with nothing to say.
    let (ok, _) = ask(&session, &["session", "notify", &ws]);
    assert!(!ok, "an empty notification was accepted");

    drop(client);
}

/// A terminal attached to one pane, with no interface around it.
///
/// The dirk client under `Client` attaches to the whole session; this one runs
/// `dirk pane attach`, which is a different program on the far side and has to
/// be driven the same way — on a real pty, because what it produces is escape
/// sequences for one.
struct Attached {
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    screen: Arc<Mutex<vt100::Parser>>,
    rows: u16,
}

impl Attached {
    fn to(session: &str, pane: &str, takeover: bool) -> Self {
        let (cols, rows) = (COLS, ROWS);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_dirk"));
        cmd.args(["--session", session, "pane", "attach", pane]);
        if takeover {
            cmd.arg("--takeover");
        }
        cmd.cwd(env!("CARGO_MANIFEST_DIR"));
        cmd.env("TERM", "xterm-256color");
        cmd.env("XDG_CONFIG_HOME", config_home());
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
            rows,
        }
    }

    fn drawn(&self) -> String {
        let s = self.screen.lock().unwrap();
        (0..self.rows)
            .map(|r| s.screen().contents_between(r, 0, r, u16::MAX))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn wait_for(&self, needle: &str, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if self.drawn().contains(needle) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write");
        self.writer.flush().expect("flush");
    }

    fn ended(&mut self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }
}

impl Drop for Attached {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn one_pane_in_your_own_terminal_with_nothing_around_it() {
    // Attaching gives you the whole of dirk. Sometimes what you want is the one
    // pane the agent is in: over ssh from a phone, inside another multiplexer,
    // or in a terminal too small for a sidebar to be anything but in the way.
    let session = unique("attach");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    let (ok, out) = ask(&session, &["pane", "run", &pane, "printf 'zzBEFORE\\n'"]);
    assert!(ok, "pane run failed: {out}");
    assert!(
        appears_in_pane(&session, &pane, "zzBEFORE", START),
        "the pane never printed"
    );

    // What was already on the screen arrives first: attaching to a quiet pane
    // should show you what is in it, not an empty terminal.
    let mut direct = Attached::to(&session, &pane, false);
    assert!(
        direct.wait_for("zzBEFORE", START),
        "the current screen did not arrive\n{}",
        direct.drawn()
    );
    // And nothing of the interface came with it.
    assert!(
        !direct.drawn().contains("spaces"),
        "the sidebar came too:\n{}",
        direct.drawn()
    );

    // Typing goes to that pane and nowhere else.
    direct.send(b"printf 'zzTYPED'\r");
    assert!(
        direct.wait_for("zzTYPED", START),
        "what was typed never came back\n{}",
        direct.drawn()
    );
    assert!(
        appears_in_pane(&session, &pane, "zzTYPED", START),
        "the session did not see what the direct attach typed"
    );

    // One writer at a time. Two terminals typing into one shell is not a
    // feature anybody asked for.
    let refused = Attached::to(&session, &pane, false);
    assert!(
        refused.wait_for("takeover", START),
        "a second attachment was not refused\n{}",
        refused.drawn()
    );
    drop(refused);

    // Unless it says so, which is what somebody whose other terminal is already
    // closed has to be able to do.
    let mut taken = Attached::to(&session, &pane, true);
    assert!(
        taken.wait_for("zzTYPED", START),
        "the takeover did not attach\n{}",
        taken.drawn()
    );
    taken.send(b"printf 'zzTOOK'\r");
    assert!(
        appears_in_pane(&session, &pane, "zzTOOK", START),
        "the terminal that took over could not type"
    );

    // The prefix keeps its meaning, and `d` leaves.
    taken.send(b"\x00d");
    assert!(
        taken.ended(START),
        "prefix-d did not detach\n{}",
        taken.drawn()
    );
    // The pane it was showing is still running, which is the difference between
    // leaving and quitting here as everywhere else in dirk.
    let (ok, still) = ask(&session, &["pane", "read", &pane, "10"]);
    assert!(ok, "the pane died with the terminal watching it: {still}");

    drop(direct);
    drop(client);
}

#[test]
fn attaching_to_a_pane_that_does_not_exist_says_so_on_a_normal_screen() {
    // Before the terminal is put into raw mode and cleared: "no such pane" is a
    // thing to read, and a message painted onto an alternate screen that is
    // then torn down is a message nobody sees.
    let session = unique("attachbad");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, said) = ask(&session, &["pane", "attach", "p9999"]);
    assert!(!ok, "attaching to a pane that does not exist succeeded");
    assert!(
        said.contains("no such pane"),
        "the refusal did not say why: {said}"
    );

    drop(client);
}

#[test]
fn a_direct_attach_can_read_what_has_gone_past_and_typing_comes_back() {
    // A pane being read from the past looks exactly like a program that has
    // stopped, so the way back has to be something you would do anyway.
    let session = unique("attachscroll");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    // More than a screenful, so there is a past to be in.
    let (ok, out) = ask(
        &session,
        &[
            "pane",
            "run",
            &pane,
            "for i in $(seq 1 60); do printf 'zzLINE%s\\n' \"$i\"; done",
        ],
    );
    assert!(ok, "pane run failed: {out}");
    assert!(
        appears_in_pane(&session, &pane, "zzLINE60", START),
        "the pane never filled"
    );

    let mut direct = Attached::to(&session, &pane, false);
    assert!(
        direct.wait_for("zzLINE60", START),
        "the current screen did not arrive\n{}",
        direct.drawn()
    );
    assert!(
        !direct.drawn().contains("zzLINE1\n"),
        "the first line was still on screen; there is no past to scroll into"
    );

    // Page up reaches it.
    direct.send(b"\x1b[5~");
    assert!(
        direct.wait_for("zzLINE1", START),
        "page up did not move through the scrollback\n{}",
        direct.drawn()
    );

    // And typing brings you back to the live screen, because sending a
    // keystroke to a program whose output you cannot see is the kind of thing
    // you find out about afterwards.
    direct.send(b"printf 'zzBACK'\r");
    assert!(
        direct.wait_for("zzBACK", START),
        "typing did not return to the bottom\n{}",
        direct.drawn()
    );

    drop(direct);
    drop(client);
}

#[test]
fn a_harness_can_be_replaced_by_a_file_about_that_harness() {
    // Fixing one marker used to be an edit to the file that also holds your
    // projects, your boards and your theme — and there was no way to hand
    // somebody the fix.
    let session = unique("rules");
    let dir = config_home().join("cfg-rules");
    std::fs::create_dir_all(dir.join("dirk").join("agents")).expect("agents dir");
    std::fs::write(
        dir.join("dirk").join("agents").join("claude.toml"),
        "names = [\"claude\", \"zzclaudewrapper\"]\n\
         [blocked]\n\
         menu = false\n\
         match = [\"zzASKING\"]\n",
    )
    .expect("rule file");
    // And one that does not parse, which must be complained about and ignored
    // rather than taking the session with it.
    std::fs::write(
        dir.join("dirk").join("agents").join("broken.toml"),
        "names = [unclosed\n",
    )
    .expect("broken rule file");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, rules) = ask(&session, &["agent", "rules"]);
    assert!(ok, "agent rules failed: {rules}");
    assert!(
        rules.contains("zzclaudewrapper"),
        "the file did not replace the shipped rules: {rules}"
    );
    assert!(
        rules.contains("\"from\": \"file\""),
        "the answer does not say where the rules came from: {rules}"
    );
    // Replaced whole, not merged: somebody overriding claude's markers does not
    // want to inherit half of ours.
    assert!(
        !rules.contains("Would you like"),
        "the shipped markers were merged into the file's: {rules}"
    );
    // The harnesses the file said nothing about are untouched.
    assert!(rules.contains("codex"), "a harness went missing: {rules}");
    // And the file that does not parse is not a harness.
    assert!(
        !rules.contains("\"name\": \"broken\""),
        "a rule file that does not parse became a harness: {rules}"
    );

    drop(client);
}

#[test]
fn an_agent_block_in_the_config_still_works_and_a_file_wins() {
    // `[[agent]]` is what these were called first, and somebody's config should
    // not stop working because a better place to put it arrived.
    let session = unique("rulesboth");
    let dir = config_home().join("cfg-rulesboth");
    std::fs::create_dir_all(dir.join("dirk").join("agents")).expect("agents dir");
    std::fs::write(
        dir.join("dirk").join("config.toml"),
        "[[agent]]\nname = \"zzfromconfig\"\n\n[[agent]]\nname = \"claude\"\nnames = [\"zzfromblock\"]\n",
    )
    .expect("config");
    std::fs::write(
        dir.join("dirk").join("agents").join("claude.toml"),
        "names = [\"zzfromfile\"]\n",
    )
    .expect("rule file");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, rules) = ask(&session, &["agent", "rules"]);
    assert!(ok, "agent rules failed: {rules}");
    assert!(
        rules.contains("zzfromconfig"),
        "an [[agent]] block stopped working: {rules}"
    );
    // A file is the more specific statement: a document about that one harness.
    assert!(
        rules.contains("zzfromfile") && !rules.contains("zzfromblock"),
        "the block beat the file about the same harness: {rules}"
    );

    drop(client);
}

#[test]
fn a_state_that_is_wrong_can_be_asked_why() {
    // Four ranked signals decide a state and `agent list` names only the
    // winner. When the answer is wrong that is not enough: what you need is
    // what the other three said, and which marker did or did not match.
    let session = unique("explain");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = first_id(&list);

    // Nothing has reported and nothing is a harness, so every signal should say
    // so rather than the answer being a bare "unknown".
    let (ok, said) = ask(&session, &["agent", "explain", &ws]);
    assert!(ok, "agent explain failed: {said}");
    assert!(
        said.contains("\"signals\""),
        "no signals in the answer: {said}"
    );
    assert!(
        said.contains("no hook has reported here"),
        "the reported signal did not say why it was silent: {said}"
    );
    assert!(
        said.contains("not a harness dirk recognises"),
        "it did not say the pane holds no agent: {said}"
    );

    // A report puts a state on the row, and explain says which signal did it.
    let (ok, out) = ask(&session, &["agent", "state", "blocked", &ws]);
    assert!(ok, "the report was refused: {out}");
    let (ok, said) = ask(&session, &["agent", "explain", &ws]);
    assert!(ok, "agent explain failed: {said}");
    assert!(
        said.contains("\"why\": \"reported\""),
        "it did not name the winning signal: {said}"
    );
    assert!(
        said.contains("the agent said so"),
        "the winning signal did not say what it claimed: {said}"
    );

    let (ok, _) = ask(&session, &["agent", "explain", "w9999"]);
    assert!(!ok, "explaining a workspace that does not exist succeeded");

    drop(client);
}

#[test]
fn a_captured_screen_can_be_run_through_the_same_rule() {
    // This is how a wrong detection becomes a test case rather than a bug
    // report with a screenshot in it — and it works with no session at all,
    // because the screen has already been captured.
    let session = unique("explainfile");
    let dir = config_home().join("explain-screens");
    std::fs::create_dir_all(&dir).expect("screens dir");

    // A marker with a menu under it: blocked.
    let asking = dir.join("asking.txt");
    std::fs::write(&asking, "Do you want to proceed?\n  1. Yes\n  2. No\n").expect("screen");
    let (ok, said) = ask(
        &session,
        &[
            "agent",
            "explain",
            "--file",
            asking.to_str().unwrap(),
            "--agent",
            "claude",
        ],
    );
    assert!(ok, "explaining a captured screen failed: {said}");
    assert!(
        said.contains("\"blocked\": true"),
        "it did not read as blocked: {said}"
    );
    assert!(
        said.contains("\"marker\": \"Do you want\""),
        "it did not say which marker matched: {said}"
    );

    // The same words with no menu under them: a finished turn, not a question.
    // This is the rule that had to be got right, and the one worth being able
    // to demonstrate.
    let prose = dir.join("prose.txt");
    std::fs::write(&prose, "Would you like me to run the tests?\n").expect("screen");
    let (ok, said) = ask(
        &session,
        &[
            "agent",
            "explain",
            "--file",
            prose.to_str().unwrap(),
            "--agent",
            "claude",
        ],
    );
    assert!(ok, "explaining a captured screen failed: {said}");
    assert!(
        said.contains("\"blocked\": false"),
        "prose read as blocked: {said}"
    );
    assert!(
        said.contains("no menu of numbered answers"),
        "it did not say why it was not blocked: {said}"
    );

    // No session was needed for any of that.
    let socket = std::env::temp_dir()
        .join(format!("dirk-{}", unsafe { libc::getuid() }))
        .join(format!("{session}.sock"));
    assert!(!socket.exists(), "explaining a file started a session");

    // A harness dirk does not know is refused, with the ones it does listed.
    let (ok, said) = ask(
        &session,
        &[
            "agent",
            "explain",
            "--file",
            prose.to_str().unwrap(),
            "--agent",
            "nosuch",
        ],
    );
    assert!(!ok, "an unknown harness was accepted: {said}");
    assert!(
        said.contains("claude"),
        "the refusal did not list what it knows: {said}"
    );
}

#[test]
fn an_agent_behind_a_sandbox_is_still_an_agent() {
    // Detection starts from the foreground process, so an agent under a
    // sandbox or a container shim shows the wrapper and no agent at all — no
    // state, no naming, no notification, in exactly the setup where an agent is
    // most likely to be left running unattended.
    let session = unique("wrapper");
    let dir = config_home().join("cfg-wrapper");
    std::fs::create_dir_all(dir.join("dirk").join("agents")).expect("agents dir");
    // `sh -c` stands in for a sandbox: it runs something else and stays in the
    // process table as the foreground group's leader, which is the whole shape
    // of the problem. `env` and `nice` would not do — they exec, so the wrapper
    // is gone by the time anybody looks.
    std::fs::write(
        dir.join("dirk").join("config.toml"),
        "wrappers = [\"sh\"]\n",
    )
    .expect("config");
    std::fs::write(
        dir.join("dirk").join("agents").join("claude.toml"),
        "argv = [\"zzWRAPPED\"]\n",
    )
    .expect("rule file");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    // The trailing `true` is load-bearing: given one command `sh -c` execs it,
    // and then there is no wrapper left to see.
    let (ok, out) = ask(
        &session,
        &["pane", "run", &pane, "sh -c 'zzWRAPPED=1; sleep 20; true'"],
    );
    assert!(ok, "pane run failed: {out}");

    let deadline = Instant::now() + START;
    let mut seen = false;
    while Instant::now() < deadline && !seen {
        seen = ask(&session, &["agent", "list"]).1.contains("claude");
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        seen,
        "the agent behind the wrapper was not found\n{}",
        ask(&session, &["agent", "explain", &pane]).1
    );

    // And explain says it was found behind something, rather than leaving
    // somebody to wonder how dirk came to recognise a program called `env`.
    let (ok, said) = ask(&session, &["agent", "explain", &pane]);
    assert!(ok, "agent explain failed: {said}");
    assert!(
        said.contains("behind sh"),
        "explain did not say what it was found behind: {said}"
    );

    drop(client);
}

#[test]
fn a_program_that_was_not_named_a_wrapper_keeps_its_own_identity() {
    // The closed list is the point. dirk reads a command line to disambiguate a
    // program that has told it nothing, and for nothing else — so a program
    // nobody named stays what it is even when an agent's name is in its
    // arguments.
    let session = unique("nowrapper");
    let dir = config_home().join("cfg-nowrapper");
    std::fs::create_dir_all(dir.join("dirk").join("agents")).expect("agents dir");
    std::fs::write(
        dir.join("dirk").join("agents").join("claude.toml"),
        "argv = [\"zzUNWRAPPED\"]\n",
    )
    .expect("rule file");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    let (ok, out) = ask(
        &session,
        &["pane", "run", &pane, "sh -c 'zzUNWRAPPED=1; sleep 5; true'"],
    );
    assert!(ok, "pane run failed: {out}");

    // Long enough for two samples, so one that was going to match would have.
    std::thread::sleep(Duration::from_secs(3));
    let (ok, agents) = ask(&session, &["agent", "list"]);
    assert!(ok, "agent list failed");
    assert!(
        !agents.contains("claude"),
        "argv was read for a program nobody named a wrapper: {agents}"
    );

    drop(client);
}

/// Run dirk with a home directory of its own, so a test can install into one.
fn at_home(home: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", config_home())
        .output()
        .expect("run dirk");
    let said = match out.status.success() {
        true => String::from_utf8_lossy(&out.stdout).into_owned(),
        false => String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    (out.status.success(), said)
}

#[test]
fn the_hook_is_installed_rather_than_printed_at_you() {
    // The README has always said rank one is worth installing and then asked
    // you to install it by hand, so most people run on rank two for ever — and
    // dirk spends its life inferring a state its agent could have told it.
    let home = config_home().join("hookhome");
    let settings = home.join(".claude").join("settings.json");
    let _ = std::fs::remove_file(&settings);
    std::fs::create_dir_all(settings.parent().unwrap()).expect("home");

    // Somebody's file, with their setting and their hook in it.
    std::fs::write(
        &settings,
        "{\n  \"model\": \"opus\",\n  \"hooks\": {\n    \"Stop\": [\n      { \"hooks\": [{ \"type\": \"command\", \"command\": \"npm test\" }] }\n    ]\n  }\n}\n",
    )
    .expect("settings");

    let (ok, said) = at_home(&home, &["agent", "hooks", "status"]);
    assert!(ok, "hooks status failed: {said}");
    assert!(
        said.contains("absent"),
        "it did not say nothing was installed: {said}"
    );

    let (ok, said) = at_home(&home, &["agent", "hooks", "install", "claude"]);
    assert!(ok, "install failed: {said}");
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).expect("read")).expect("json");

    // Their setting and their hook are still there.
    assert_eq!(after.get("model"), Some(&serde_json::json!("opus")));
    assert!(
        std::fs::read_to_string(&settings)
            .unwrap()
            .contains("npm test"),
        "installing removed somebody else's hook"
    );

    // Twice is a no-op, and says so rather than adding a second copy.
    let (ok, said) = at_home(&home, &["agent", "hooks", "install", "claude"]);
    assert!(ok, "installing twice failed: {said}");
    assert!(
        said.contains("already"),
        "it did not say it was already there: {said}"
    );
    let (ok, said) = at_home(&home, &["agent", "hooks", "status"]);
    assert!(ok && said.contains("installed"), "status is wrong: {said}");

    // And out again, leaving what was there.
    let (ok, said) = at_home(&home, &["agent", "hooks", "uninstall", "claude"]);
    assert!(ok, "uninstall failed: {said}");
    let text = std::fs::read_to_string(&settings).expect("read");
    assert!(
        text.contains("npm test"),
        "uninstalling took somebody else's hook: {text}"
    );
    assert!(
        !text.contains("dirk agent state"),
        "uninstalling left ours: {text}"
    );
    assert!(text.contains("opus"), "uninstalling lost a setting: {text}");
}

#[test]
fn a_hook_somebody_edited_is_reported_rather_than_overwritten() {
    // The person who edited it is exactly the person who would never trust this
    // again if it were silently replaced.
    let home = config_home().join("hookedited");
    let settings = home.join(".claude").join("settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).expect("home");
    std::fs::write(
        &settings,
        "{\"hooks\":{\"Stop\":[{\"hooks\":[{\"type\":\"command\",\
         \"command\":\"dirk agent state done --current && say done\"}]}]}}",
    )
    .expect("settings");

    let (ok, said) = at_home(&home, &["agent", "hooks", "status"]);
    assert!(ok, "hooks status failed: {said}");
    assert!(
        said.contains("edited"),
        "an edited hook was not noticed: {said}"
    );

    let (ok, said) = at_home(&home, &["agent", "hooks", "install", "claude"]);
    assert!(!ok, "it overwrote an edited hook: {said}");
    let text = std::fs::read_to_string(&settings).expect("read");
    assert!(
        text.contains("say done"),
        "somebody's edit did not survive a refused install: {text}"
    );
}

#[test]
fn a_harness_with_no_snippet_says_so_rather_than_pretending() {
    let home = config_home().join("hooknone");
    std::fs::create_dir_all(&home).expect("home");
    let (ok, said) = at_home(&home, &["agent", "hooks", "install", "aider"]);
    assert!(!ok, "it claimed to install a hook it does not have: {said}");
    assert!(
        said.contains("no snippet"),
        "the refusal did not say why: {said}"
    );
    // And the printed form still works, which is what that harness has.
    let (ok, said) = at_home(&home, &["agent", "hooks", "aider"]);
    assert!(ok, "printing a snippet failed: {said}");
    assert!(
        said.contains("dirk agent state"),
        "nothing to paste: {said}"
    );
}

#[test]
fn a_restored_session_comes_back_on_the_conversation_it_was_having() {
    // Of everything in this milestone, the one that changes how the tool feels.
    // Restoring the shape of the work while losing the work is close to the
    // worst available place to stop.
    let session = unique("resume");
    let dir = config_home().join("cfg-resume");
    std::fs::create_dir_all(dir.join("dirk").join("agents")).expect("agents dir");
    // A stand-in harness: `printf` is not an agent, but what is being tested is
    // that dirk starts the right command line with the right reference in it.
    std::fs::write(
        dir.join("dirk").join("agents").join("zzharness.toml"),
        "names = [\"zzharness\"]\nresume = [\"printf\", \"zzRESUMED-{session}\"]\n",
    )
    .expect("rule file");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = first_id(&list);
    // What a hook would report: a state, and the harness's own name for the
    // conversation.
    let (ok, said) = ask(
        &session,
        &["agent", "state", "done", &ws, "--session", "conv-42"],
    );
    assert!(ok, "the report was refused: {said}");

    // The pane has to be holding that harness for the reference to be kept
    // against it — a reference is only meaningful to the program that issued
    // it. Reported again once the harness is what the pane is running.
    let (ok, panes) = ask(&session, &["pane", "list", &ws]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    // A process whose name is the harness, made by giving `sleep` that name.
    // `exec -a` would be shorter and is a bashism: CI's /bin/sh is dash, where
    // it fails, takes the pane's shell with it, and ends the session.
    let bin = config_home().join("zzharness-bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    let link = bin.join("zzharness");
    if !link.exists() {
        let sleep = ["/bin/sleep", "/usr/bin/sleep"]
            .iter()
            .map(std::path::Path::new)
            .find(|p| p.exists())
            .expect("a sleep to borrow a name from");
        std::os::unix::fs::symlink(sleep, &link).expect("symlink");
    }
    let running = format!("{} 30", link.display());
    let (ok, out) = ask(&session, &["pane", "run", &pane, &running]);
    assert!(ok, "pane run failed: {out}");
    let deadline = Instant::now() + START;
    while Instant::now() < deadline {
        let (_, said) = ask(
            &session,
            &["agent", "state", "done", &ws, "--session", "conv-42"],
        );
        if said.contains("conv-42") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let (ok, said) = ask(
        &session,
        &["agent", "state", "done", &ws, "--session", "conv-42"],
    );
    assert!(ok, "the report was refused: {said}");
    assert!(
        said.contains("conv-42"),
        "the reference was not kept against the harness: {said}"
    );

    // Down and up again. The processes are gone; the conversation is not.
    drop(client);
    end(&session);
    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = first_id(&list);
    let (ok, panes) = ask(&session, &["pane", "list", &ws]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    assert!(
        appears_in_pane(&session, &pane, "zzRESUMED-conv-42", START),
        "the agent did not come back on its conversation\n{}",
        ask(&session, &["pane", "read", &pane, "20"]).1
    );

    drop(client);
}

#[test]
fn a_harness_dirk_cannot_resume_comes_back_as_a_shell() {
    // Every restored pane used to be a shell, and one that stays a shell has
    // lost nothing it had a moment ago. A reference dirk cannot use is
    // therefore not an error.
    let session = unique("noresume");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let ws = first_id(&list);
    // aider ships with no resume template, which is the case being tested.
    let (ok, said) = ask(
        &session,
        &["agent", "state", "done", &ws, "--session", "conv-99"],
    );
    assert!(ok, "the report was refused: {said}");

    drop(client);
    end(&session);
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    // It came back, and it came back usable.
    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    let (ok, out) = ask(&session, &["pane", "run", &pane, "printf zzSHELL"]);
    assert!(ok, "pane run failed: {out}");
    assert!(
        appears_in_pane(&session, &pane, "zzSHELL", START),
        "the restored pane is not a working shell"
    );

    drop(client);
}

/// A pane holding a process named after a harness.
///
/// A symlink rather than `exec -a`, which is a bashism: CI's /bin/sh is dash,
/// where it fails and takes the pane's shell with it.
fn pretend_to_be(session: &str, pane: &str, harness: &str) {
    let bin = config_home().join(format!("{harness}-bin"));
    std::fs::create_dir_all(&bin).expect("bin dir");
    let link = bin.join(harness);
    if !link.exists() {
        let sleep = ["/bin/sleep", "/usr/bin/sleep"]
            .iter()
            .map(std::path::Path::new)
            .find(|p| p.exists())
            .expect("a sleep to borrow a name from");
        std::os::unix::fs::symlink(sleep, &link).expect("symlink");
    }
    let running = format!("{} 30", link.display());
    let (ok, out) = ask(session, &["pane", "run", pane, &running]);
    assert!(ok, "pane run failed: {out}");
    let deadline = Instant::now() + START;
    while Instant::now() < deadline {
        if ask(session, &["agent", "list"]).1.contains(harness) {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("{harness} was never recognised in {pane}");
}

/// Report a blocked claude in a workspace nobody is looking at, and say whether
/// the machine with the speakers made a noise about it.
fn rings_for_claude(name: &str, says: &str) -> bool {
    let session = unique(name);
    let dir = config_home().join(format!("cfg-{name}"));
    std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
    let rang = dir.join("rang");
    let _ = std::fs::remove_file(&rang);
    std::fs::write(
        dir.join("dirk").join("config.toml"),
        format!(
            "[sound]\nenabled = true\nblocked = [\"sh\", \"-c\", \"touch {}\"]\n\n\
             [sound.agents]\nclaude = \"{says}\"\n",
            rang.display()
        ),
    )
    .expect("config");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    // Somewhere that is not on screen, because a workspace you are looking at
    // is one you already know about.
    let (ok, _) = ask(&session, &["workspace", "create"]);
    assert!(ok, "workspace create failed");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let all = ids(&list);
    let ws = all.last().expect("a second workspace").clone();
    let (ok, _) = ask(&session, &["workspace", "focus", &all[0]]);
    assert!(ok, "workspace focus failed");

    let (ok, panes) = ask(&session, &["pane", "list", &ws]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    pretend_to_be(&session, &pane, "claude");

    let (ok, out) = ask(&session, &["agent", "state", "blocked", &ws]);
    assert!(ok, "the report was refused: {out}");

    // The state reaches the column either way; only the noise is in question.
    let (ok, agents) = ask(&session, &["agent", "list"]);
    assert!(
        ok && agents.contains("blocked"),
        "the state did not reach the column: {agents}"
    );

    let heard = match says {
        // Waited for when it should ring, and waited out when it should not.
        "on" => appears(&rang, START),
        _ => {
            std::thread::sleep(Duration::from_secs(3));
            rang.exists()
        }
    };
    drop(client);
    heard
}

#[test]
fn one_chatty_harness_can_be_silenced_without_silencing_the_others() {
    // The shape of the problem when three are running: one is chatty and the
    // other two are not, and the answers available were "all" and "none" — or
    // silencing the whole repository, which silences the two you wanted to
    // hear. Both directions, so this cannot pass because nothing rings for some
    // unrelated reason.
    assert!(
        !rings_for_claude("soundoff", "off"),
        "the harness that asked to be quiet made a noise"
    );
    assert!(
        rings_for_claude("soundon", "on"),
        "the harness that asked to be heard was silent"
    );
}

#[test]
fn something_to_show_does_not_have_to_become_a_state_to_be_visible() {
    // `agent state` is a small closed set dirk reasons about — waits,
    // notifications, ordering, the attention column — and it has to stay that
    // way. Anything a program wanted to *show* had nowhere else to go, which is
    // how an indexer's progress ends up interrupting somebody.
    let session = unique("said");
    let dir = config_home().join("cfg-said");
    std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
    std::fs::write(
        dir.join("dirk").join("config.toml"),
        "[naming]\ntemplates = [\"{intent} {said.summary}\"]\n",
    )
    .expect("config");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);

    let (ok, said) = ask(&session, &["pane", "metadata", &pane, "summary=zzINDEXING"]);
    assert!(ok, "pane metadata failed: {said}");
    assert!(said.contains("zzINDEXING"), "it did not read back: {said}");

    // It is display and nothing else: the state, the ordering and what is owed
    // are all exactly as they were.
    let (ok, agents) = ask(&session, &["agent", "list"]);
    assert!(ok, "agent list failed");
    assert!(
        !agents.contains("zzINDEXING"),
        "a display token reached the agent list: {agents}"
    );

    // And it is available to a template, which is what a row shows. Naming only
    // decides a label when there is an intent to decide one from, so the pane
    // has to say what it is doing first — which is the only situation in which
    // a template runs at all.
    let (ok, out) = ask(
        &session,
        &["pane", "run", &pane, "printf '\\033]2;zzWORKING\\007'"],
    );
    assert!(ok, "pane run failed: {out}");
    assert!(
        client.wait_for("zzINDEXING", START),
        "the token never reached the column\n{}",
        client.drawn()
    );

    // An empty value clears rather than setting an empty one: absent and empty
    // render the same and mean different things.
    let (ok, said) = ask(&session, &["pane", "metadata", &pane, "summary="]);
    assert!(ok, "clearing failed: {said}");
    assert!(
        !said.contains("zzINDEXING"),
        "clearing left it behind: {said}"
    );

    // Nonsense is refused rather than stored.
    let (ok, _) = ask(&session, &["pane", "metadata", &pane, "nokeyvalue"]);
    assert!(!ok, "a token that is not key=value was accepted");

    drop(client);
}

#[test]
fn what_was_said_about_a_pane_does_not_survive_the_process_it_described() {
    // These describe a moment in a process that is gone after a restart, and
    // restoring them would be restoring a claim nobody is making any more.
    let session = unique("saidgone");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    let (ok, out) = ask(&session, &["pane", "metadata", &pane, "summary=zzGONE"]);
    assert!(ok, "pane metadata failed: {out}");

    drop(client);
    end(&session);
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    let (ok, said) = ask(&session, &["pane", "metadata", &pane]);
    assert!(ok, "pane metadata failed: {said}");
    assert!(
        !said.contains("zzGONE"),
        "what was said about a dead process came back: {said}"
    );

    drop(client);
}

/// A stand-in for a full-screen agent: alternate screen on, mouse reporting on,
/// and a transcript it redraws itself when it is scrolled.
///
/// Written out by the test rather than checked in because it is scaffolding for
/// one thing, and because what it has to be is exactly what the feature reads —
/// a program whose history is inside it and reachable only by asking.
fn a_transcript_program() -> std::path::PathBuf {
    let path = config_home().join("zztranscript.py");
    std::fs::write(
        &path,
        r#"
import sys, os, tty, termios
# Raw mode, as any full-screen program does: without it the pty echoes what
# dirk writes and the transcript comes back with the scrolling in it.
tty.setraw(sys.stdin.fileno())
out = sys.stdout
top = 0
def draw():
    out.write("\x1b[H\x1b[2J")
    for i in range(20):
        out.write("zzLINE%03d\r\n" % (top + i))
    out.write("zzEND")
    out.flush()
out.write("\x1b[?1049h\x1b[?1000h")
draw()
# X10 mouse reports: ESC [ M Cb Cx Cy, with Cb 96 for a wheel up and 97 down.
fd = sys.stdin.fileno()
while True:
    b = os.read(fd, 1)
    if not b:
        break
    if b[0] == 96:
        top = max(0, top - 1); draw()
    elif b[0] == 97:
        top = top + 1; draw()
"#,
    )
    .expect("write the stand-in");
    path
}

#[test]
fn an_agent_that_keeps_its_history_to_itself_can_still_be_read() {
    // A full-screen agent draws its transcript in the alternate screen, so
    // dirk's scrollback holds none of it — none of it ever scrolled. A read of
    // two hundred lines quietly returned twenty, with nothing to say that the
    // rest existed.
    let session = unique("transcript");
    let dir = config_home().join("cfg-transcript");
    std::fs::create_dir_all(dir.join("dirk").join("agents")).expect("agents dir");
    std::fs::write(
        dir.join("dirk").join("agents").join("zztranscript.toml"),
        "argv = [\"zztranscript\"]\n",
    )
    .expect("rule file");
    let program = a_transcript_program();

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    let (ok, out) = ask(
        &session,
        &[
            "pane",
            "run",
            &pane,
            &format!("python3 {}", program.display()),
        ],
    );
    assert!(ok, "pane run failed: {out}");

    // Wait until dirk sees the harness, which is what makes this path apply.
    let deadline = Instant::now() + START;
    let mut seen = false;
    while Instant::now() < deadline && !seen {
        seen = ask(&session, &["agent", "list"]).1.contains("zztranscript");
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(seen, "the stand-in was never recognised as an agent");

    // Wait until it has stopped drawing. A screen being redrawn under this
    // would be stitched out of two different moments, which is the reason the
    // read refuses a working agent at all.
    let (ok, out) = ask(
        &session,
        &[
            "agent",
            "wait",
            &pane,
            "--until",
            "idle",
            "--until",
            "done",
            "--timeout",
            "30000",
        ],
    );
    assert!(ok, "the agent never settled: {out}");

    // A read that fits on the screen is the read it always was, and moves
    // nothing.
    let (ok, said) = ask(&session, &["pane", "read", &pane, "10"]);
    assert!(ok, "the ordinary read failed: {said}");
    assert!(said.contains("zzEND"), "the ordinary read is wrong: {said}");

    // A read of more than the screen holds goes and gets it.
    let (ok, said) = ask(&session, &["pane", "read", &pane, "60"]);
    assert!(ok, "the transcript read failed: {said}");
    assert!(
        said.contains("zzLINE000"),
        "it did not reach the top of the transcript: {said}"
    );
    assert!(
        said.contains("zzEND"),
        "it lost the bottom of the transcript: {said}"
    );
    // Stitched, not concatenated: every line appears once.
    let twice = said.matches("zzLINE005").count();
    assert_eq!(twice, 1, "a line appeared {twice} times: {said}");

    // And the agent was put back where it was, so the next person to look at
    // it is not looking at its past.
    let deadline = Instant::now() + START;
    let mut back = false;
    while Instant::now() < deadline && !back {
        back = ask(&session, &["pane", "read", &pane, "5"])
            .1
            .contains("zzEND");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        back,
        "the read left the agent scrolled into its own history\n{}",
        ask(&session, &["pane", "read", &pane, "25"]).1
    );

    drop(client);
}

#[test]
fn nothing_else_moves_an_agents_viewport() {
    // The narrowness is the feature. A pane that is not an agent, or one that
    // is being read from the past, is refused rather than driven — and a read
    // that fits on the screen never takes this path at all.
    let session = unique("noscroll");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_id(&panes);
    let program = a_transcript_program();
    let (ok, out) = ask(
        &session,
        &[
            "pane",
            "run",
            &pane,
            &format!("python3 {}", program.display()),
        ],
    );
    assert!(ok, "pane run failed: {out}");
    // Long enough for the program to be drawing and for dirk to have sampled.
    std::thread::sleep(Duration::from_secs(3));

    // No rules name this one, so dirk holds no agent here — and a read that
    // would have to drive an unrecognised program says so instead.
    let (ok, said) = ask(&session, &["pane", "read", &pane, "60"]);
    assert!(!ok, "it drove a program it does not recognise: {said}");
    assert!(
        said.contains("holds no agent"),
        "the refusal did not say why: {said}"
    );

    // A read inside the screen is unaffected.
    let (ok, said) = ask(&session, &["pane", "read", &pane, "10"]);
    assert!(ok, "an ordinary read was refused: {said}");
    assert!(said.contains("zzEND"), "the ordinary read is wrong: {said}");

    drop(client);
}

#[test]
fn a_pane_starts_the_shell_and_the_directory_that_were_asked_for() {
    // A non-login shell on macOS never reads the files that build a login PATH
    // — path_helper and Homebrew's initialisation — so PATH inside a dirk pane
    // was missing entries it has in every other terminal on the machine. That
    // reads as dirk being broken.
    let session = unique("shellmode");
    let dir = config_home().join("cfg-shellmode");
    std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
    let elsewhere = config_home().join("zzelsewhere");
    std::fs::create_dir_all(&elsewhere).expect("a directory to start in");
    std::fs::write(
        dir.join("dirk").join("config.toml"),
        format!(
            "[terminal]\nshell_mode = \"login\"\nnew_cwd = \"{}\"\n",
            elsewhere.display()
        ),
    )
    .expect("config");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let first = first_id(&panes);

    // A pane made beside another one starts where the policy says, not where
    // the one it came from was.
    let (ok, out) = ask(&session, &["pane", "split", &first, "rows"]);
    assert!(ok, "split failed: {out}");
    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    assert!(
        panes.contains("zzelsewhere"),
        "the new pane did not start where it was told: {panes}"
    );
    // And the one that was already there is untouched.
    assert!(
        panes.matches("zzelsewhere").count() == 1,
        "the policy reached a pane that already existed: {panes}"
    );

    // The shell is a login shell, which is the whole point of the setting.
    let ids = ids(&panes);
    let made = ids.last().expect("the new pane").clone();
    let (ok, out) = ask(&session, &["pane", "run", &made, "printf 'zz%s\\n' \"$0\""]);
    assert!(ok, "pane run failed: {out}");
    // `-l` is what dirk passes and what a shell reports back in `$0` only
    // sometimes, so ask the shell itself whether it is one.
    let (ok, out) = ask(
        &session,
        &[
            "pane",
            "run",
            &made,
            "case $- in *l*) printf zzLOGIN;; *) printf zzPLAIN;; esac",
        ],
    );
    assert!(ok, "pane run failed: {out}");
    assert!(
        appears_in_pane(&session, &made, "zzLOGIN", START),
        "the pane's shell is not a login shell\n{}",
        ask(&session, &["pane", "read", &made, "20"]).1
    );

    drop(client);
}

#[test]
fn a_shell_mode_nobody_understands_is_complained_about() {
    let session = unique("shellbad");
    let dir = config_home().join("cfg-shellbad");
    std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
    std::fs::write(
        dir.join("dirk").join("config.toml"),
        "[terminal]\nshell_mode = \"loginish\"\n",
    )
    .expect("config");

    let client = Client::with_config(&session, &dir);
    assert!(client.wait_for(READY, START), "never started");
    // A session still starts: a setting nobody understands falls back rather
    // than costing you the session it was in.
    let (ok, out) = ask(&session, &["session", "reload"]);
    assert!(ok, "reload failed: {out}");
    assert!(
        out.contains("not auto, login or non_login"),
        "the reload did not say what it did not understand: {out}"
    );

    drop(client);
}

#[test]
fn a_pane_moves_and_takes_its_process_with_it() {
    // A pane was created in a workspace and stayed there. Splitting was the
    // only way to organise, so a workspace holding logs, a server, a test
    // watcher and an agent was four panes competing for one screen.
    let session = unique("movepane");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let first = first_id(&panes);
    let (ok, out) = ask(&session, &["pane", "split", &first, "cols"]);
    assert!(ok, "split failed: {out}");
    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let moving = ids(&panes).last().expect("the new pane").clone();

    // Something running in it, and something in its scrollback, so the move can
    // be seen to have carried both.
    let (ok, out) = ask(&session, &["pane", "run", &moving, "printf 'zzCARRIED\\n'"]);
    assert!(ok, "pane run failed: {out}");
    assert!(
        appears_in_pane(&session, &moving, "zzCARRIED", START),
        "the pane never printed"
    );

    // Out into a workspace of its own.
    let (ok, said) = ask(&session, &["pane", "move", &moving, "--new-workspace"]);
    assert!(ok, "the move failed: {said}");
    assert!(
        said.contains("\"previous\""),
        "it did not say where it came from: {said}"
    );

    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    assert_eq!(ids(&list).len(), 2, "no second workspace: {list}");

    // The scrollback came with it, and so did the shell: the pane is the same
    // pane, and dirk's ids are session-wide so its own handle did not change.
    assert!(
        ask(&session, &["pane", "read", &moving, "20"])
            .1
            .contains("zzCARRIED"),
        "the scrollback did not travel"
    );
    let (ok, out) = ask(&session, &["pane", "run", &moving, "printf 'zzALIVE\\n'"]);
    assert!(ok, "pane run failed after the move: {out}");
    assert!(
        appears_in_pane(&session, &moving, "zzALIVE", START),
        "the process did not survive the move"
    );

    // And into a tab of its own, back where it started.
    let (ok, tabs) = ask(&session, &["tab", "list"]);
    assert!(ok, "tab list failed");
    let home = first_id(&tabs);
    let (ok, said) = ask(&session, &["pane", "move", &moving, "--tab", &home]);
    assert!(ok, "the move back failed: {said}");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    assert_eq!(
        ids(&list).len(),
        1,
        "the workspace it left behind was not closed: {list}"
    );

    drop(client);
}

#[test]
fn a_move_that_cannot_happen_is_refused_rather_than_attempted() {
    let session = unique("movebad");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, panes) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let only = first_id(&panes);

    // A workspace's only pane has nowhere to go that is not where it is, and
    // taking it out would close the workspace under it.
    let (ok, said) = ask(&session, &["pane", "move", &only, "--new-workspace"]);
    assert!(!ok, "it moved a workspace's only pane out: {said}");
    let (ok, said) = ask(&session, &["pane", "move", &only, "--tab", "w1:t99"]);
    assert!(!ok, "it moved a pane to a tab that does not exist: {said}");
    let (ok, said) = ask(&session, &["pane", "move", &only]);
    assert!(!ok, "it moved a pane nowhere in particular: {said}");

    // And the pane is still there and still works.
    let (ok, out) = ask(&session, &["pane", "run", &only, "printf 'zzSTILL\\n'"]);
    assert!(ok, "pane run failed: {out}");
    assert!(
        appears_in_pane(&session, &only, "zzSTILL", START),
        "a refused move disturbed the pane"
    );

    drop(client);
}

#[test]
fn what_a_pane_last_said_can_come_back_with_it() {
    // The shape without the output is a set of empty shells. What was on the
    // screen -- the error, the summary you had not copied -- is usually the
    // reason you were coming back at all.
    let home = config_home().join("history-on");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("dirk")).expect("config dir");
    std::fs::write(
        home.join("dirk").join("config.toml"),
        "[session]\npane_history = true\n",
    )
    .expect("config");

    let session = unique("history");
    let spawn = || {
        let home = home.clone();
        Client::spawn(&session, COLS, ROWS, &move |cmd| {
            cmd.env("XDG_CONFIG_HOME", &home);
        })
    };

    let client = spawn();
    assert!(client.wait_for(READY, START), "never started");
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");
    let (ok, why) = ask(&session, &["pane", "run", &pane, "printf", "zzTRACE"]);
    assert!(ok, "pane run failed: {why}");
    assert!(
        client.wait_for("zzTRACE", START),
        "the pane never printed it\n{}",
        client.drawn()
    );

    // Written when the session is, so give it a shape change to write on.
    let (ok, _) = ask(&session, &["workspace", "rename", &first_id(&list), "kept"]);
    assert!(ok, "rename failed");
    drop(client);
    end(&session);

    let back = spawn();
    assert!(back.wait_for(READY, START), "did not come back");
    assert!(
        back.wait_for("zzTRACE", START),
        "what the pane last said did not come back\n{}",
        back.drawn()
    );
    // And it says where the past stops, or it is a screenful pretending to be
    // live -- which is the thing this was argued against being.
    assert!(
        back.drawn().contains("history"),
        "nothing marked it as history\n{}",
        back.drawn()
    );

    drop(back);
    end(&session);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_pane_says_nothing_from_before_unless_it_was_asked_to() {
    // Off, and off is the default: a pane's output holds tokens and keys and
    // whatever was in the environment when something printed a debug line.
    let session = unique("nohistory");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");
    let (ok, why) = ask(&session, &["pane", "run", &pane, "printf", "zzSECRET"]);
    assert!(ok, "pane run failed: {why}");
    assert!(
        client.wait_for("zzSECRET", START),
        "the pane never printed it"
    );
    let (ok, wslist) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let (ok, _) = ask(
        &session,
        &["workspace", "rename", &first_id(&wslist), "kept"],
    );
    assert!(ok, "rename failed");
    drop(client);
    end(&session);

    // Nothing on disk, and nothing on the screen.
    let stored = config_home()
        .join("dirk")
        .join("sessions")
        .join(format!("{session}.history.json"));
    assert!(
        !stored.exists(),
        "a history file was written with the setting off: {}",
        stored.display()
    );

    let back = Client::attach(&session);
    assert!(back.wait_for(READY, START), "did not come back");
    assert!(
        back.wait_until(START, |c| c.drawn().contains("kept")),
        "the session did not come back at all\n{}",
        back.drawn()
    );
    assert!(
        !back.drawn().contains("zzSECRET"),
        "output came back with the setting off\n{}",
        back.drawn()
    );

    drop(back);
    end(&session);
}

#[test]
fn turning_the_history_off_takes_away_what_was_already_kept() {
    // Otherwise "off" describes the future and not the file, which is not what
    // somebody turning it off is asking for.
    let home = config_home().join("history-off");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("dirk")).expect("config dir");
    let cfg = home.join("dirk").join("config.toml");
    std::fs::write(&cfg, "[session]\npane_history = true\n").expect("config");

    let session = unique("histoff");
    let spawn = || {
        let home = home.clone();
        Client::spawn(&session, COLS, ROWS, &move |cmd| {
            cmd.env("XDG_CONFIG_HOME", &home);
        })
    };
    let stored = home
        .join("dirk")
        .join("sessions")
        .join(format!("{session}.history.json"));

    let client = spawn();
    assert!(client.wait_for(READY, START), "never started");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let (ok, _) = ask(&session, &["workspace", "rename", &first_id(&list), "kept"]);
    assert!(ok, "rename failed");
    drop(client);
    end(&session);
    assert!(
        stored.exists(),
        "nothing was kept with the setting on: {}",
        stored.display()
    );

    // Off, and a reason to write the session again.
    std::fs::write(&cfg, "[session]\npane_history = false\n").expect("config");
    let back = spawn();
    assert!(back.wait_for(READY, START), "did not come back");
    let (ok, list) = ask(&session, &["workspace", "list"]);
    assert!(ok, "workspace list failed");
    let (ok, _) = ask(
        &session,
        &["workspace", "rename", &first_id(&list), "later"],
    );
    assert!(ok, "rename failed");
    drop(back);
    end(&session);

    assert!(
        !stored.exists(),
        "turning it off left what was already kept: {}",
        stored.display()
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// Start a request as its own process, so it can be killed rather than waited
/// for.
///
/// `ask_later` runs on a thread, and a thread blocked in `Command::output` is
/// not something a test can abandon. A caller giving up is the whole subject
/// here, and only a process can give up.
fn ask_and_abandon(session: &str, args: &[&str]) -> std::process::Child {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"));
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
    ] {
        cmd.env_remove(var);
    }
    cmd.args(["--session", session])
        .args(args)
        .env("XDG_CONFIG_HOME", config_home())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("run dirk")
}

/// How many questions the session is holding.
fn waiting(session: &str) -> usize {
    let (ok, out) = ask(session, &["session", "info"]);
    assert!(ok, "session info failed: {out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    v["waiting"].as_u64().expect("a waiting count") as usize
}

/// Poll until it is true, or give up and say what it was.
fn until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("{what}");
}

#[test]
fn a_caller_that_gives_up_is_not_waited_for() {
    // `pane wait-output` with no `--timeout` waits for as long as it takes,
    // which is what it is for -- and a caller that gave up used to leave the
    // session holding the question for the rest of its life, re-deciding it on
    // every turn of the loop, with a thread and a socket parked behind it.
    //
    // Ctrl-C on a script is the ordinary way to produce this, and a script
    // written to time itself out produces it every time it does.
    let session = unique("gaveup");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = ids(&list).first().expect("a pane").clone();
    assert_eq!(waiting(&session), 0, "something was already waiting");

    let mut gone = ask_and_abandon(
        &session,
        &["pane", "wait-output", &pane, "zzz-never-happens-zzz"],
    );
    until("the wait was never taken up", || waiting(&session) == 1);

    // The caller gives up. Nothing tells the session; it has to notice.
    let _ = gone.kill();
    let _ = gone.wait();
    until("the session kept a question nobody was waiting for", || {
        waiting(&session) == 0
    });
}

#[test]
fn a_handoff_replaces_the_binary_and_keeps_every_pane_running() {
    // Upgrading meant ending every shell and every agent, so people did not
    // upgrade while they were working -- which is always.
    let home = config_home().join("handoff-on");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("dirk")).expect("config dir");
    std::fs::write(
        home.join("dirk").join("config.toml"),
        "[session]\nhandoff = true\n",
    )
    .expect("config");

    let session = unique("handoff");
    let client = Client::spawn(&session, COLS, ROWS, &{
        let home = home.clone();
        move |cmd| {
            cmd.env("XDG_CONFIG_HOME", &home);
        }
    });
    assert!(client.wait_for(READY, START), "never started");

    // Something that will still be there afterwards if the pty survived, and
    // will not be if the pane was restarted.
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");
    let (ok, why) = ask(&session, &["pane", "run", &pane, "printf", "zzBEFORE"]);
    assert!(ok, "pane run failed: {why}");
    assert!(client.wait_for("zzBEFORE", START), "the pane never printed");

    let before = pane_pid(&session, &pane);
    assert!(before > 0, "no process behind the pane");

    let (ok, why) = ask(&session, &["session", "handoff"]);
    // Never ok: on success this process becomes the new binary partway through
    // and there is nobody left to answer.
    assert!(!ok, "the handoff answered, which means it did not happen");
    // "did not answer" is what success looks like from out here: the process
    // became the new binary partway through the call, so the connection closed
    // with no reply. Anything else is a refusal, and a refusal names a reason.
    assert!(
        why.contains("did not answer"),
        "the handoff was refused rather than performed: {why}"
    );

    // The session is still there, answering, with the same process behind the
    // same pane.
    let deadline = Instant::now() + START;
    let mut after = 0;
    while Instant::now() < deadline {
        after = pane_pid(&session, &pane);
        if after > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(
        after, before,
        "the pane's process changed across the handoff: it was restarted, not kept"
    );

    // And it is the same pty: the shell still has the history it had.
    let (ok, why) = ask(&session, &["pane", "run", &pane, "printf", "zzAFTER"]);
    assert!(ok, "the pane stopped working after the handoff: {why}");
    let (ok, text) = ask(&session, &["pane", "read", &pane, "50"]);
    assert!(ok, "pane read failed");
    assert!(
        text.contains("zzBEFORE"),
        "what was on the screen before the handoff did not come across:\n{text}"
    );

    drop(client);
    end(&session);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_handoff_is_refused_until_it_is_asked_for() {
    // The thing at risk is every running pane in the session, which is the
    // most expensive thing dirk holds.
    let session = unique("nohandoff");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");

    let (ok, why) = ask(&session, &["session", "handoff"]);
    assert!(!ok, "a handoff happened without being asked for");
    assert!(
        why.contains("experimental") && why.contains("handoff"),
        "the refusal did not say how to turn it on: {why}"
    );
    // And the session is untouched.
    let (ok, _) = ask(&session, &["session", "info"]);
    assert!(ok, "the refused handoff disturbed the session");

    drop(client);
    end(&session);
}

/// The process behind a pane, or zero.
fn pane_pid(session: &str, pane: &str) -> i64 {
    let (ok, list) = ask(session, &["pane", "list"]);
    if !ok {
        return 0;
    }
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&list) else {
        return 0;
    };
    doc.get("panes")
        .and_then(|p| p.as_array())
        .and_then(|panes| {
            panes.iter().find(|p| {
                p.get("id")
                    .and_then(|i| i.as_str())
                    .is_some_and(|i| i == pane || pane.ends_with(i) || i.ends_with(pane))
            })
        })
        .and_then(|p| p.get("pid").and_then(|v| v.as_i64()))
        .unwrap_or(0)
}

#[test]
fn a_pane_can_be_watched_as_data_by_more_than_one_reader() {
    // `pane read` answers at a moment, so following a pane meant polling and
    // rendering one was impossible: the escapes are stripped and there is no
    // stream. Observers do not own the pane, so a recorder and a bridge can
    // both watch what somebody is typing into.
    let session = unique("observe");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");

    let watchers: Vec<_> = (0..2)
        .map(|_| {
            std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
                .args(["--session", &session, "pane", "observe", &pane])
                .env("XDG_CONFIG_HOME", config_home())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn observer")
        })
        .collect();

    std::thread::sleep(Duration::from_millis(600));
    let (ok, why) = ask(&session, &["pane", "run", &pane, "printf", "zzSTREAM"]);
    assert!(ok, "pane run failed: {why}");
    std::thread::sleep(Duration::from_secs(2));

    for (i, mut w) in watchers.into_iter().enumerate() {
        let _ = w.kill();
        let out = w.wait_with_output().expect("observer output");
        let text = String::from_utf8_lossy(&out.stdout);
        let mut opened = false;
        let mut saw = false;
        for line in text.lines() {
            let doc: serde_json::Value = match serde_json::from_str(line) {
                Ok(d) => d,
                Err(e) => panic!("observer {i} wrote a line that is not JSON: {line:?} ({e})"),
            };
            if doc.get("open").is_some() {
                opened = true;
            }
            if let Some(b64) = doc.get("bytes").and_then(|v| v.as_str()) {
                saw |= decode_b64(b64).windows(8).any(|w| w == b"zzSTREAM");
            }
        }
        assert!(opened, "observer {i} never got an opening record:\n{text}");
        assert!(saw, "observer {i} never saw what the pane printed:\n{text}");
    }

    // And the pane is still somebody's to type in: observing took nothing.
    let (ok, _) = ask(&session, &["pane", "run", &pane, "printf", "zzAFTER"]);
    assert!(ok, "the pane was taken away by watching it");
    assert!(
        client.wait_for("zzAFTER", START),
        "the pane stopped working after being observed\n{}",
        client.drawn()
    );

    drop(client);
    end(&session);
}

/// Base64, for reading what the stream carries.
fn decode_b64(text: &str) -> Vec<u8> {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut have = 0u32;
    let mut out = Vec::new();
    for c in text.bytes().filter(|c| *c != b'=') {
        let Some(v) = A.iter().position(|a| *a == c) else {
            continue;
        };
        bits = (bits << 6) | v as u32;
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
        }
    }
    out
}

#[test]
fn a_pane_can_be_driven_from_outside_by_one_caller_at_a_time() {
    // Bytes out and bytes in, which is what somebody building a different front
    // end needs -- and one writer, because two things typing into one shell
    // interleave characters and the program gets the blame.
    use std::io::Write;
    let session = unique("control");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");

    let mut driver = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--session", &session, "pane", "control", &pane])
        .env("XDG_CONFIG_HOME", config_home())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn controller");
    std::thread::sleep(Duration::from_millis(600));

    // A second controller is refused while the first holds it.
    let second = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--session", &session, "pane", "control", &pane])
        .env("XDG_CONFIG_HOME", config_home())
        .output()
        .expect("second controller");
    assert!(
        !second.status.success(),
        "two controllers were allowed at once"
    );
    let said = String::from_utf8_lossy(&second.stderr);
    assert!(
        said.contains("takeover"),
        "the refusal did not say how to replace it: {said}"
    );

    // Typing through the stream reaches the program.
    let mut stdin = driver.stdin.take().expect("stdin");
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"input": "printf zzDRIVEN\r"})
    )
    .expect("write");
    stdin.flush().expect("flush");
    assert!(
        client.wait_for("zzDRIVEN", START),
        "what was typed through the stream never reached the pane\n{}",
        client.drawn()
    );

    // And letting go hands it back, so the next caller is not refused.
    writeln!(stdin, "{}", serde_json::json!({"release": true})).expect("write");
    stdin.flush().expect("flush");
    drop(stdin);
    let _ = driver.wait();
    std::thread::sleep(Duration::from_millis(600));
    let mut third = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--session", &session, "pane", "observe", &pane])
        .env("XDG_CONFIG_HOME", config_home())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn after release");
    std::thread::sleep(Duration::from_millis(600));
    let _ = third.kill();
    let out = third.wait_with_output().expect("output");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("\"open\""),
        "the pane was not given back: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    drop(client);
    end(&session);
}

#[test]
fn a_reader_that_stops_reading_is_dropped_rather_than_holding_the_session() {
    // One stalled observer must not be able to stop the loop that owns every
    // other pane. The write timeout turns a full pipe into an error and the
    // watcher is dropped on it; this is the test that says so, because the
    // failure it prevents is the whole session going quiet.
    let session = unique("slow");
    let client = Client::attach(&session);
    assert!(client.wait_for(READY, START), "never started");
    let (ok, list) = ask(&session, &["pane", "list"]);
    assert!(ok, "pane list failed");
    let pane = first_field(&list, "id");

    let watcher = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--session", &session, "pane", "observe", &pane])
        .env("XDG_CONFIG_HOME", config_home())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn observer");
    std::thread::sleep(Duration::from_millis(600));

    // Stopped, not killed: a killed process closes its socket and is dropped
    // immediately, which is the easy case. A stopped one holds the connection
    // open and never drains it, which is the case that could stall the loop.
    unsafe { libc::kill(watcher.id() as i32, libc::SIGSTOP) };

    // Enough output to fill anything buffering it.
    for _ in 0..40 {
        let (ok, _) = ask(&session, &["pane", "run", &pane, "seq", "1", "500"]);
        assert!(
            ok,
            "the session stopped answering while a reader was stalled"
        );
    }

    // The session is still answering, promptly, with a stalled reader attached.
    let began = Instant::now();
    let (ok, out) = ask(&session, &["session", "info"]);
    assert!(ok, "the session stopped answering: {out}");
    assert!(
        began.elapsed() < Duration::from_secs(10),
        "answering took {:?} with one stalled reader",
        began.elapsed()
    );

    unsafe { libc::kill(watcher.id() as i32, libc::SIGKILL) };
    let mut watcher = watcher;
    let _ = watcher.wait();
    drop(client);
    end(&session);
}
