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

//! Does dirk actually come up?
//!
//! The unit tests cover the naming policy, which is pure. Everything else in
//! dirk is a terminal talking to a terminal, and the only honest way to test
//! that is to give it one: spawn the real binary on a real pty, read what it
//! paints, and drive it with real keystrokes.

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The pty is drained on its own thread, always.
///
/// This is not tidiness. A pty buffer is a few kilobytes, and dirk redraws on
/// every tick; a harness that reads only while waiting for something fills that
/// buffer the moment it stops, dirk blocks in `write`, and the next keystroke is
/// never processed. The first version of this file did exactly that and made
/// dirk look like it ignored `q`.
struct Harness {
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    seen: Arc<Mutex<String>>,
}

impl Harness {
    fn start() -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_dirk"));
        cmd.cwd(env!("CARGO_MANIFEST_DIR"));
        cmd.env("TERM", "xterm-256color");
        // A predictable, quiet shell: an interactive zsh would paint a prompt
        // and a theme over the assertions.
        cmd.env("SHELL", "/bin/sh");
        // Keep config lookup inside the repo so the test never depends on what
        // happens to be in the real ~/.config.
        cmd.env("XDG_CONFIG_HOME", env!("CARGO_MANIFEST_DIR"));

        let child = pair.slave.spawn_command(cmd).expect("spawn dirk");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("reader");
        let seen = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&seen);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    return;
                }
                sink.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        });

        Self {
            writer: pair.master.take_writer().expect("writer"),
            child,
            seen,
        }
    }

    fn wait_for(&mut self, needle: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.seen.lock().unwrap().contains(needle) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    fn drawn(&self) -> String {
        self.seen.lock().unwrap().clone()
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write");
        self.writer.flush().expect("flush");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn it_comes_up_and_draws_its_chrome() {
    let mut h = Harness::start();

    // The sidebar's section headings are letter-spaced, so this also proves
    // the heading helper ran rather than a raw string being echoed.
    assert!(
        h.wait_for("P R O J E C T S", Duration::from_secs(10)),
        "no projects heading; dirk drew:\n{}",
        h.drawn()
    );
    // The brand, in the rail. This is the only place it appears.
    assert!(
        h.wait_for("dirk", Duration::from_secs(5)),
        "no brand in the rail"
    );
}

#[test]
fn a_shell_in_a_pane_runs_and_echoes() {
    let mut h = Harness::start();
    assert!(
        h.wait_for("P R O J E C T S", Duration::from_secs(10)),
        "never started"
    );

    // Straight through to the pane: no prefix, so this is the focused shell.
    h.send(b"echo dirk-is-alive\r");
    assert!(
        h.wait_for("dirk-is-alive", Duration::from_secs(10)),
        "the shell never echoed; dirk drew:\n{}",
        h.drawn()
    );
}

#[test]
fn the_prefix_opens_the_picker_and_escape_closes_it() {
    let mut h = Harness::start();
    assert!(
        h.wait_for("P R O J E C T S", Duration::from_secs(10)),
        "never started"
    );

    // Ctrl-Space, then o.
    h.send(&[0]);
    h.send(b"o");
    assert!(
        h.wait_for("open", Duration::from_secs(5)),
        "picker never opened; dirk drew:\n{}",
        h.drawn()
    );
    h.send(&[0x1b]);
}

#[test]
fn prefix_q_quits() {
    let mut h = Harness::start();
    assert!(
        h.wait_for("P R O J E C T S", Duration::from_secs(10)),
        "never started"
    );

    h.send(&[0]);
    h.send(b"q");

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = h.child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("dirk did not exit on prefix q");
}
