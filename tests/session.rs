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

fn unique(kind: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    format!(
        "test-{kind}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
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

/// Ask the session to end, and wait for its socket to go.
fn quit(session: &str) {
    let mut last = Client::attach(session);
    if last.wait_for(READY, START) {
        last.send(&[0]);
        std::thread::sleep(Duration::from_millis(150));
        last.send(b"q");
    }
    let socket = std::env::temp_dir()
        .join(format!("dirk-{}", unsafe { libc::getuid() }))
        .join(format!("{session}.sock"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = std::fs::remove_file(&socket);
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
    quit(&session);
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
    quit(&session);
}

#[test]
fn a_second_client_takes_over_and_the_first_is_told_why() {
    let session = unique("takeover");

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

    // The first is told, rather than simply going quiet.
    assert!(
        first.wait_for("taken over", Duration::from_secs(10)),
        "the first client was dropped without a word\n{}",
        first.drawn()
    );
    drop(second);
    quit(&session);
}

#[test]
fn a_server_nobody_can_reach_does_not_keep_running() {
    // A socket can go without the server going with it. What is left holds
    // every shell in the session and nothing can ever attach to it again, so
    // it should end rather than become something only `ps` can find.
    let session = unique("unreachable");
    let client = Client::attach(&session);
    assert!(
        client.wait_for(READY, START),
        "never started\n{}",
        client.drawn()
    );

    let socket = std::env::temp_dir()
        .join(format!("dirk-{}", unsafe { libc::getuid() }))
        .join(format!("{session}.sock"));
    assert!(socket.exists(), "no socket to remove");
    std::fs::remove_file(&socket).expect("remove the socket");

    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let still_there = std::process::Command::new("ps")
            .args(["-A", "-o", "args="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&session))
            .unwrap_or(false);
        if !still_there {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("the server outlived every way of reaching it");
}

#[test]
fn a_client_that_took_over_is_the_one_being_drawn_for() {
    // Taking over is only half of it. The client that was replaced closes its
    // socket on the way out, and an unnamed detach would clear the view of the
    // one that replaced it -- leaving the new client attached to a session that
    // had quietly stopped drawing for it.
    let session = unique("still-drawing");

    let first = Client::attach(&session);
    assert!(
        first.wait_for(READY, START),
        "never started\n{}",
        first.drawn()
    );

    let mut second = Client::attach(&session);
    assert!(
        second.wait_for(READY, START),
        "the second never drew\n{}",
        second.drawn()
    );
    assert!(
        first.wait_for("taken over", Duration::from_secs(10)),
        "the first was not told\n{}",
        first.drawn()
    );
    drop(first);
    std::thread::sleep(Duration::from_millis(500));

    // The point: the survivor still works.
    second.send(b"printf 'zz%s' AFTER\r");
    assert!(
        second.wait_for("zzAFTER", Duration::from_secs(10)),
        "the client that took over was frozen\n{}",
        second.drawn()
    );
    drop(second);
    quit(&session);
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
    quit(&session);
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
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--session", session])
        .args(args)
        .env("XDG_CONFIG_HOME", config_home())
        .output()
        .expect("run dirk");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
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

    let (ok, _) = ask(&session, &["pane", "send-keys", &id, "printf zzASKED\n"]);
    assert!(ok, "send-keys failed");

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
    quit(&session);
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
            .any(|l| l.starts_with(&session) && l.contains("attached")),
        "the session with a client attached was not listed as attached:\n{listed}"
    );

    drop(client);
    quit(&session);

    // And once it is gone it is not offered as something to attach to.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["session", "list"])
        .output()
        .expect("run dirk");
    let listed = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        !listed
            .lines()
            .any(|l| l.starts_with(&session) && (l.contains("running") || l.contains("attached"))),
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
    quit(&session);
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
        .env("DIRK_SESSION", &session)
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
    quit(&session);
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
    quit(&session);

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
    quit(&session);
}

// ── Over a link that is not a socket ────────────────────────────────────

/// A stand-in for ssh: drops `-T -- <target>` and runs the rest here.
///
/// `flaky` makes the first link die two seconds in, which is what a laptop lid
/// looks like from this side.
fn fake_ssh(name: &str, flaky: bool) -> std::path::PathBuf {
    let path = config_home().join(format!("ssh-{name}"));
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
    std::fs::write(&path, body).expect("write the stand-in");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let _ = std::fs::remove_file(config_home().join(format!("mark-{name}")));
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
    quit(&session);
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
    quit(&session);
}
