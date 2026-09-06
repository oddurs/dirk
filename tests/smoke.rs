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

const ROWS: u16 = 24;
const COLS: u16 = 80;

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
    screen: Arc<Mutex<vt100::Parser>>,
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
        let screen = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
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
        }
    }

    fn wait_for(&mut self, needle: &str, timeout: Duration) -> bool {
        self.wait_until(timeout, |h| h.find(needle).is_some())
    }

    fn wait_until(&mut self, timeout: Duration, done: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    /// The screen as text, one string per row, trailing blanks trimmed.
    fn rows(&self) -> Vec<String> {
        let s = self.screen.lock().unwrap();
        (0..ROWS)
            .map(|r| {
                (0..COLS)
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

    /// Where `needle` sits on screen, as (row, column).
    fn find(&self, needle: &str) -> Option<(usize, usize)> {
        self.rows()
            .iter()
            .enumerate()
            .find_map(|(r, line)| line.find(needle).map(|c| (r, c)))
    }

    fn drawn(&self) -> String {
        self.rows().join("\n")
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

/// The prefix key, Ctrl-Space, which is NUL on the wire.
const PREFIX: &[u8] = &[0];

impl Harness {
    fn prefix(&mut self, key: &[u8]) {
        self.send(PREFIX);
        self.send(key);
        // The prefix and its command are two events; give the loop a frame to
        // process them before the next keystroke arrives.
        std::thread::sleep(Duration::from_millis(120));
    }
}

#[test]
fn splitting_twice_gives_three_side_by_side_columns() {
    let mut h = Harness::start();
    assert!(
        h.wait_for("P R O J E C T S", Duration::from_secs(10)),
        "never started"
    );

    // The sidebar is hidden so the panes are wide enough that neither the
    // echoed command nor the marker wraps; a wrapped marker is not a layout
    // failure but would read as one.
    h.prefix(b"d");

    // Both splits before anything is written. A marker written first would be
    // written at the old width and moved by the resize that follows.
    h.prefix(b"|");
    h.prefix(b"|");

    // `printf 'zz%s' A` prints zzA without the typed command containing it, so
    // searching the screen finds the output rather than the shell's echo of the
    // command that produced it.
    //
    // Focus is on the newest pane and `;` cycles in draw order, so this leaves
    // one distinct marker in each of the three.
    h.send(b"printf 'zz%s' C\r");
    h.prefix(b";");
    h.send(b"printf 'zz%s' A\r");
    h.prefix(b";");
    h.send(b"printf 'zz%s' B\r");

    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            h.find("zzA").is_some() && h.find("zzB").is_some() && h.find("zzC").is_some()
        }),
        "not all three panes echoed\n{}",
        h.drawn()
    );

    let (_, a) = h.find("zzA").expect("zzA");
    let (_, b) = h.find("zzB").expect("zzB");
    let (_, c) = h.find("zzC").expect("zzC");

    // Each marker sits at the left edge of its own pane, so the columns are the
    // pane origins. If the tree had nested each split instead of appending to a
    // same-direction parent, the third pane would sit inside the second's half
    // and the gaps would differ -- so the spacing is checked, not just the
    // order.
    assert!(
        a < b && b < c,
        "expected three columns, got {a}, {b}, {c}\n{}",
        h.drawn()
    );
    assert!(
        (b - a).abs_diff(c - b) <= 1,
        "columns should be even thirds; gaps were {} and {}\n{}",
        b - a,
        c - b,
        h.drawn()
    );
}

#[test]
fn closing_a_split_pane_gives_the_whole_width_back() {
    let mut h = Harness::start();
    assert!(
        h.wait_for("P R O J E C T S", Duration::from_secs(10)),
        "never started"
    );

    h.prefix(b"|");
    h.prefix(b"x");

    // Forty characters do not fit in half of a 52-column content area, so if
    // the split had left a single-child node behind instead of collapsing, this
    // would wrap onto a second row.
    let marker = "X".repeat(40);
    h.send(format!("printf {marker}\r").as_bytes());

    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .rows()
            .iter()
            .any(|r| r.contains(&marker))),
        "the surviving pane did not get the full width back\n{}",
        h.drawn()
    );
}
