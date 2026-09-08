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

/// dirk has come up when the nav has drawn. The `+ workspace` row is the
/// signal rather than a heading, because it appears only in the nav — the rail
/// carries the word "spaces" too, and matching that would pass before the nav
/// had drawn anything.
const READY: &str = "+ workspace";

/// How long to allow for dirk to come up.
///
/// Generous on purpose. Starting is a process spawn, a pty, a shell and a first
/// frame, and on a loaded CI runner that is not instant — a tight bound here
/// fails as "never started" and reads like a bug in dirk.
const START: Duration = Duration::from_secs(30);

/// Unique per config directory, so concurrent tests do not share one.
fn next_config_id() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

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
    /// Every window title dirk has asked the outer terminal for, in order.
    titles: Arc<Mutex<Vec<String>>>,
    /// Kept so a test can narrow the terminal. Most of what the rail does is
    /// decide what to give up as it runs out of room, and that cannot be
    /// tested at one width.
    master: Box<dyn portable_pty::MasterPty + Send>,
}

/// Take the complete window titles out of a byte stream.
///
/// `ESC ] 0 ;` and `ESC ] 2 ;` both set one — 0 is icon-and-window and is what
/// crossterm writes — and either ends at BEL or at an escape. Whatever is left
/// of a partial one stays in the buffer, because a title can arrive across two
/// reads.
fn take_titles(buf: &mut Vec<u8>) -> Vec<String> {
    let opener = |w: &[u8]| w == b"\x1b]0;" || w == b"\x1b]2;";
    const OPEN: &[u8] = b"\x1b]0;";
    let mut out = Vec::new();
    loop {
        let Some(start) = buf.windows(OPEN.len()).position(opener) else {
            // Keep only enough to recognise an opener split across two reads.
            if buf.len() > OPEN.len() {
                buf.drain(..buf.len() - OPEN.len());
            }
            return out;
        };
        let body = start + OPEN.len();
        let Some(end) = buf[body..].iter().position(|b| *b == 0x07 || *b == 0x1b) else {
            buf.drain(..start);
            return out;
        };
        out.push(String::from_utf8_lossy(&buf[body..body + end]).into_owned());
        buf.drain(..body + end + 1);
    }
}

/// What every test in this file wants on top of whatever it configures.
///
/// Non-login shells, for the same reason the harness sets `SHELL` and points
/// `XDG_CONFIG_HOME` away from home: these tests read what is drawn, and a
/// login shell brings /etc/profile, /etc/bashrc and somebody's prompt into it.
/// Two of those write a window title after every command — which is an intent,
/// which is a workspace label, and one of these tests is about what a *test*
/// put in that title.
const ISOLATED: &str = "[terminal]\nshell_mode = \"non_login\"\n";

impl Harness {
    fn start() -> Self {
        Self::start_with_config("")
    }

    /// Start dirk against a written configuration file.
    ///
    /// Each call gets its own config directory, so a test that needs a layout
    /// does not change what every other test sees.
    fn start_with_config(config: &str) -> Self {
        let dir = std::env::temp_dir().join("dirk-smoke").join(format!(
            "{}-{}",
            std::process::id(),
            next_config_id()
        ));
        std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
        std::fs::write(
            dir.join("dirk").join("config.toml"),
            // Appended, not prepended: a section header before somebody's
            // top-level keys would swallow them into it, the file would be
            // refused, and the test would silently run on the defaults.
            format!("{config}\n{ISOLATED}"),
        )
        .expect("config");
        Self::start_with(Some(dir))
    }

    fn start_with(config_home: Option<std::path::PathBuf>) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_dirk"));
        // One process, no session. These tests are about the multiplexer
        // rather than about how it is reached, and a shared session would mean
        // every one of them attaching to whichever server started first --
        // with whichever configuration that one was given.
        cmd.arg("--no-session");
        cmd.cwd(env!("CARGO_MANIFEST_DIR"));
        cmd.env("TERM", "xterm-256color");
        // A predictable, quiet shell: an interactive zsh would paint a prompt
        // and a theme over the assertions.
        cmd.env("SHELL", "/bin/sh");
        // Keep config lookup away from the real ~/.config, so a test never
        // depends on what happens to be in the author's own configuration.
        cmd.env(
            "XDG_CONFIG_HOME",
            config_home.unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
        );

        let child = pair.slave.spawn_command(cmd).expect("spawn dirk");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("reader");
        let screen = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
        let sink = Arc::clone(&screen);
        // vt100 drops window titles unless a callback claims them, and what
        // dirk writes to the terminal *it* is in never reaches a pane's parser
        // at all. Kept as raw bytes and read back below.
        let titles = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&titles);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut carry = Vec::new();
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    return;
                }
                sink.lock().unwrap().process(&buf[..n]);
                carry.extend_from_slice(&buf[..n]);
                for title in take_titles(&mut carry) {
                    seen.lock().unwrap().push(title);
                }
            }
        });

        Self {
            writer: pair.master.take_writer().expect("writer"),
            child,
            screen,
            titles,
            master: pair.master,
        }
    }

    /// The last window title dirk asked for, if it asked for one.
    fn title(&self) -> Option<String> {
        self.titles.lock().unwrap().last().cloned()
    }

    /// Narrow the terminal, and the parser with it.
    ///
    /// Both, or the screen keeps reporting the old width and every row reads
    /// as padded with whatever was there before the resize.
    fn resize(&mut self, cols: u16) {
        self.master
            .resize(PtySize {
                rows: ROWS,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("resize");
        self.screen
            .lock()
            .unwrap()
            .screen_mut()
            .set_size(ROWS, cols);
        std::thread::sleep(Duration::from_millis(600));
    }

    /// One row, at whatever width the terminal currently is.
    fn row(&self, y: u16, cols: u16) -> String {
        let screen = self.screen.lock().unwrap();
        (0..cols)
            .map(|x| {
                screen
                    .screen()
                    .cell(y, x)
                    .map_or(" ", |c| c.contents())
                    .to_string()
            })
            .collect::<String>()
            .trim_end()
            .to_string()
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
    ///
    /// The column is counted in characters, not bytes. dirk's own chrome is
    /// full of multi-byte glyphs — `▾ ◆ ▏ ▊` — and so is the output of
    /// anything running in a pane, so a byte offset is not a column and using
    /// one would shift an assertion by two for every glyph to its left.
    fn find(&self, needle: &str) -> Option<(usize, usize)> {
        self.rows().iter().enumerate().find_map(|(r, line)| {
            line.find(needle)
                .map(|byte| (r, line[..byte].chars().count()))
        })
    }

    fn drawn(&self) -> String {
        self.rows().join("\n")
    }

    /// A left click at a cell, in the SGR encoding dirk enables.
    fn click(&mut self, col: u16, row: u16) {
        let (c, r) = (col + 1, row + 1);
        self.send(format!("\x1b[<0;{c};{r}M").as_bytes());
        self.send(format!("\x1b[<0;{c};{r}m").as_bytes());
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Press, move, release: the gesture that selects.
    ///
    /// The move matters -- a press and a release in the same cell is a click,
    /// which belongs to whatever is in the pane.
    fn drag(&mut self, from_col: u16, from_row: u16, to_col: u16, to_row: u16) {
        let (c, r) = (from_col + 1, from_row + 1);
        let (c2, r2) = (to_col + 1, to_row + 1);
        self.send(format!("\x1b[<0;{c};{r}M").as_bytes());
        std::thread::sleep(Duration::from_millis(80));
        // Button 32 is a drag with the left button held.
        self.send(format!("\x1b[<32;{c2};{r2}M").as_bytes());
        std::thread::sleep(Duration::from_millis(80));
        self.send(format!("\x1b[<0;{c2};{r2}m").as_bytes());
        std::thread::sleep(Duration::from_millis(200));
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

    // The nav's three sections, as plain words.
    assert!(
        h.wait_for(READY, START),
        "the nav never drew; dirk drew:\n{}",
        h.drawn()
    );
    // The mark, in the rail's first cells. The brand is one glyph now: it does
    // the branding job completely and in one column, and the slot beside it
    // answers where you are, which a product name cannot. Searching the screen
    // for the word would find the project in the nav and pass for the wrong
    // reason.
    assert!(
        h.wait_until(Duration::from_secs(5), |h| h
            .row(ROWS - 1, COLS)
            .trim_start()
            .starts_with('◆')),
        "no mark in the rail\n{}",
        h.drawn()
    );
}

#[test]
fn a_shell_in_a_pane_runs_and_echoes() {
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

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
    assert!(h.wait_for(READY, START), "never started");

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
    assert!(h.wait_for(READY, START), "never started");

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
    assert!(h.wait_for(READY, START), "never started");

    // The sidebar is hidden so the panes are wide enough that neither the
    // echoed command nor the marker wraps; a wrapped marker is not a layout
    // failure but would read as one.
    h.prefix(b"b");

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
    assert!(h.wait_for(READY, START), "never started");

    h.prefix(b"|");

    // A marker in the new pane, so its disappearance is the signal that the
    // close has actually been processed. Closing is not synchronous with the
    // keystroke -- the child is killed, the pty reaches EOF, and the event
    // comes back -- and typing into the gap sends the keys to the pane that is
    // on its way out, where they are lost.
    h.send(b"printf 'zz%s' GOING\r");
    assert!(
        h.wait_for("zzGOING", Duration::from_secs(10)),
        "the new pane never echoed"
    );

    h.prefix(b"x");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzGOING").is_none()),
        "the pane was never closed\n{}",
        h.drawn()
    );

    // Forty characters do not fit in half of the content area, so if the split
    // had left a single-child node behind instead of collapsing, this would
    // wrap onto a second row.
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

#[test]
fn a_layout_opens_every_pane_it_declares() {
    // Two panes stacked, each labelled. This exercises the whole path:
    // parsing the layout, planning the tree, spawning at the planned size, and
    // drawing the label rule that says which panel is which.
    let mut h = Harness::start_with_config(
        r#"
[[layout]]
name = "Test"
key = "9"
split = "rows"

[[layout.pane]]
title = "upper"
command = ["/bin/sh"]

[[layout.pane]]
title = "lower"
command = ["/bin/sh"]
"#,
    );
    assert!(h.wait_for(READY, START), "never started");

    // The configured layout replaces the built-in ones, so it is the only entry
    // in the section above the tree.
    assert!(
        h.wait_for("Test", Duration::from_secs(5)),
        "layout not listed\n{}",
        h.drawn()
    );

    h.prefix(b"9");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            h.find("upper").is_some() && h.find("lower").is_some()
        }),
        "both panes should be labelled\n{}",
        h.drawn()
    );

    let (upper, _) = h.find("upper").expect("upper");
    let (lower, _) = h.find("lower").expect("lower");
    assert!(
        upper < lower,
        "split rows should stack upper above lower\n{}",
        h.drawn()
    );
}

#[test]
fn a_layout_whose_program_is_missing_is_not_offered() {
    let mut h = Harness::start_with_config(
        r#"
[[layout]]
name = "Ghost"
key = "9"
command = ["definitely-not-installed-anywhere"]
"#,
    );
    assert!(h.wait_for(READY, START), "never started");

    // An entry that could only ever show `command not found` is worse than no
    // entry, so it is dropped at startup rather than left to fail on first use.
    assert!(
        h.find("Ghost").is_none(),
        "a layout that cannot run was listed\n{}",
        h.drawn()
    );
}

#[test]
fn panes_in_a_layout_can_be_cycled_and_typed_into() {
    // The bug this pins: `focused_workspace_mut` answered `None` for a layout,
    // so cycling did nothing and the keyboard stayed on the first leaf forever
    // -- in the one place with several panes worth moving between.
    let mut h = Harness::start_with_config(
        r#"
[[layout]]
name = "Test"
key = "9"
split = "cols"

[[layout.pane]]
title = "left"
command = ["/bin/sh"]

[[layout.pane]]
title = "right"
command = ["/bin/sh"]
"#,
    );
    assert!(h.wait_for(READY, START), "never started");
    h.prefix(b"9");
    assert!(
        h.wait_for("left", Duration::from_secs(10)),
        "layout never opened\n{}",
        h.drawn()
    );

    h.send(b"printf 'zz%s' A\r");
    h.prefix(b";");
    h.send(b"printf 'zz%s' B\r");

    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            h.find("zzA").is_some() && h.find("zzB").is_some()
        }),
        "both panes should have taken input\n{}",
        h.drawn()
    );

    let (_, a) = h.find("zzA").expect("zzA");
    let (_, b) = h.find("zzB").expect("zzB");
    assert!(
        a < b,
        "the two markers should be in different panes, got {a} and {b}\n{}",
        h.drawn()
    );
}

#[test]
fn the_nav_takes_keys_of_its_own_and_gives_them_back() {
    let mut h = Harness::start_with_config(
        r#"
[[layout]]
name = "Solo"
key = "9"
command = ["/bin/sh"]
"#,
    );
    assert!(h.wait_for(READY, START), "never started");
    assert!(
        h.wait_for("Solo", Duration::from_secs(5)),
        "layout not listed\n{}",
        h.drawn()
    );

    // Entering the nav starts on the focused workspace, so two steps up reach
    // the layout: workspace -> project -> layout.
    h.prefix(b"w");
    h.send(b"k");
    std::thread::sleep(Duration::from_millis(150));
    h.send(b"k");
    std::thread::sleep(Duration::from_millis(150));
    h.send(b"\r");

    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            // The layout is open once its own pane is running, which the dot
            // beside the name reports.
            h.rows().iter().any(|r| r.contains("• Solo"))
        }),
        "Enter in the nav did not open the layout\n{}",
        h.drawn()
    );

    // Opening something hands the keyboard back, so this reaches the shell
    // rather than being read as nav movement.
    h.send(b"printf 'zz%s' Q\r");
    assert!(
        h.wait_for("zzQ", Duration::from_secs(10)),
        "keys did not return to the pane\n{}",
        h.drawn()
    );
}

#[test]
fn escape_leaves_the_nav_without_going_anywhere() {
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.prefix(b"w");
    h.send(b"j");
    std::thread::sleep(Duration::from_millis(150));
    h.send(&[0x1b]);
    std::thread::sleep(Duration::from_millis(150));

    // `j` was nav movement and must not have reached the shell; the shell only
    // starts hearing keys again after Escape.
    h.send(b"printf 'zz%s' R\r");
    assert!(
        h.wait_for("zzR", Duration::from_secs(10)),
        "keys did not come back\n{}",
        h.drawn()
    );
    assert!(
        !h.rows().iter().any(|r| r.contains("jprintf")),
        "the nav leaked a keystroke into the pane\n{}",
        h.drawn()
    );
}

#[test]
fn a_layout_pane_holds_its_place_when_its_program_exits() {
    // The bug: a panel that printed and exited was reaped like any other pane,
    // so a dashboard of `cairn board` panels emptied itself within a second and
    // left no sign the panels had been there.
    let mut h = Harness::start_with_config(
        r#"
[[layout]]
name = "Test"
key = "9"
split = "rows"

[[layout.pane]]
title = "report"
command = ["/bin/sh", "-c", "printf 'zz%s' KEPT; exit 3"]

[[layout.pane]]
title = "alive"
command = ["/bin/sh"]
"#,
    );
    assert!(h.wait_for(READY, START), "never started");
    h.prefix(b"9");

    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("exited 3").is_some()),
        "the pane did not report how it ended\n{}",
        h.drawn()
    );
    assert!(
        h.find("zzKEPT").is_some(),
        "its output was thrown away\n{}",
        h.drawn()
    );
    assert!(
        h.find("report").is_some(),
        "its label went with it\n{}",
        h.drawn()
    );
    // The other pane is untouched, and the layout still has its shape.
    assert!(
        h.find("alive").is_some(),
        "the layout was rearranged around it\n{}",
        h.drawn()
    );
}

#[test]
fn a_stopped_layout_pane_can_still_be_closed() {
    // Holding a stopped pane introduced its own trap: `close` kills the child,
    // and killing one that has already exited produces no new event -- so the
    // pane sat there waiting to be reaped by something that was never coming.
    let mut h = Harness::start_with_config(
        r#"
[[layout]]
name = "Test"
key = "9"
split = "rows"

[[layout.pane]]
title = "report"
command = ["/bin/sh", "-c", "printf 'zz%s' KEPT; exit 3"]

[[layout.pane]]
title = "alive"
command = ["/bin/sh"]
"#,
    );
    assert!(h.wait_for(READY, START), "never started");
    h.prefix(b"9");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("exited 3").is_some()),
        "the pane never reported stopping\n{}",
        h.drawn()
    );

    // Focus is on the first leaf, which is the stopped one.
    h.prefix(b"x");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzKEPT").is_none()),
        "x did not close the stopped pane\n{}",
        h.drawn()
    );
    assert!(
        h.find("alive").is_some(),
        "closing it took the layout with it\n{}",
        h.drawn()
    );
}

#[test]
fn the_quit_button_takes_two_clicks() {
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    // The button is the last cell of the bottom row.
    let (row, col) = h
        .rows()
        .iter()
        .enumerate()
        .find_map(|(r, line)| line.find('✕').map(|c| (r, line[..c].chars().count())))
        .expect("no quit button in the rail");
    assert_eq!(row, ROWS as usize - 1, "the button belongs in the rail");

    // One click arms it and says so; it must not quit.
    h.click(col as u16, row as u16);
    assert!(
        h.wait_for("quit?", Duration::from_secs(5)),
        "one click should arm, not quit\n{}",
        h.drawn()
    );
    assert!(
        h.child.try_wait().ok().flatten().is_none(),
        "one click quit dirk"
    );

    // The second confirms.
    h.click(col as u16, row as u16);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = h.child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the second click did not quit\n{}", h.drawn());
}

#[test]
fn a_shell_that_sets_a_title_is_not_mistaken_for_an_agent() {
    // Shells set terminal titles, usually to the working directory, and the
    // first version of the agents list was "anything that has published a
    // title" -- so every plain shell was listed as something wanting attention.
    // What is running is now read from the pane's foreground process group.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf '\\033]2;Looks like an intent\\007'\r");
    assert!(
        h.wait_for("Looks like an intent", Duration::from_secs(10)),
        "the workspace was never named\n{}",
        h.drawn()
    );

    // The name is adopted -- that part is right, the title is all dirk has to
    // go on. What must not happen is the shell being marked as an agent.
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        !agent_at_rest(&h),
        "a shell was marked as an agent\n{}",
        h.drawn()
    );
}

/// A program that behaves like a shell but is named like an agent, so the whole
/// detection chain can be exercised without installing one.
fn fake_agent(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("dirk-smoke-bin").join(format!(
        "{}-{}",
        std::process::id(),
        next_config_id()
    ));
    std::fs::create_dir_all(&dir).expect("bin dir");
    let link = dir.join(name);
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink("/bin/sh", &link).expect("symlink");
    link
}

#[test]
fn a_pane_running_an_agent_is_detected_as_one() {
    // End to end: the foreground process group is read from the pty, looked up
    // in the process table, and its name classified. A shell named `claude` is
    // enough to drive all three -- what is being identified is the name of the
    // program in the foreground, which is the whole point.
    let claude = fake_agent("claude");
    let mut h =
        Harness::start_with_config(&format!("shell = {:?}\n", claude.display().to_string()));
    assert!(h.wait_for(READY, START), "never started");

    assert!(
        h.wait_until(Duration::from_secs(15), agent_at_rest),
        "a pane running `claude` was not recognised as holding an agent\n{}",
        h.drawn()
    );
}

/// Whether the nav says a workspace holds an agent that is doing nothing.
///
/// An idle agent draws a dot in the state column and a shell at a prompt draws
/// nothing, which since the agents list became a zone for what is owed is the
/// only thing on screen that tells the two apart. Matched on the workspace's
/// own row -- the dot is also the held-name mark and the footer's separator.
fn agent_at_rest(h: &Harness) -> bool {
    h.rows().iter().any(|r| r.trim_start().starts_with("· 1 "))
}

#[test]
fn an_agent_sitting_on_an_approval_prompt_reads_as_blocked() {
    // A shell named `claude` is detected as one, so printing what Claude Code
    // prints when it wants an answer drives the whole path: detection, the tail
    // of the screen, the marker table, the state machine, the glyph.
    let claude = fake_agent("claude");
    let mut h =
        Harness::start_with_config(&format!("shell = {:?}\n", claude.display().to_string()));
    assert!(h.wait_for(READY, START), "never started");

    // Recognised as an agent, and not blocked while it has said nothing.
    assert!(
        h.wait_until(Duration::from_secs(15), agent_at_rest),
        "not detected as an agent\n{}",
        h.drawn()
    );
    let blocked = |h: &Harness| h.rows().iter().any(|r| r.trim_start().starts_with("! "));
    assert!(
        !blocked(&h),
        "blocked before anything was asked\n{}",
        h.drawn()
    );

    // A question in prose is not an approval prompt -- it is how a finished
    // turn ends -- so this must not read as blocked.
    h.send(b"printf 'Would you like me to run the tests?\\n'\r");
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        !blocked(&h),
        "a question in prose read as an approval prompt\n{}",
        h.drawn()
    );

    // The real thing: a question above a menu of answers.
    h.send(b"printf 'Do you want to proceed?\\n> 1. Yes\\n  2. No\\n'\r");
    assert!(
        h.wait_until(Duration::from_secs(10), blocked),
        "an approval prompt did not read as blocked\n{}",
        h.drawn()
    );

    // And it stops being blocked once the prompt is off the screen.
    h.send(b"clear\r");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| !blocked(h)),
        "still blocked after the prompt was cleared\n{}",
        h.drawn()
    );
}

#[test]
fn work_that_finishes_while_you_are_elsewhere_reads_as_done() {
    // The seen rule, end to end. `done` and `idle` are the same underlying
    // state and only whether you were looking separates them, so the only way
    // to test it for real is to look somewhere else while work happens.
    let claude = fake_agent("claude");
    let mut h =
        Harness::start_with_config(&format!("shell = {:?}\n", claude.display().to_string()));
    assert!(h.wait_for(READY, START), "never started");
    assert!(
        h.wait_until(Duration::from_secs(15), agent_at_rest),
        "not detected as an agent\n{}",
        h.drawn()
    );

    // Work that will land after focus has moved away.
    h.send(b"(sleep 3; printf 'zz%s' LATE) &\r");
    h.prefix(b"n");

    // The new workspace is focused, so the first one is working unwatched, and
    // what it produces is unseen when it stops.
    assert!(
        h.wait_until(Duration::from_secs(20), |h| {
            h.rows().iter().any(|r| r.trim_start().starts_with("+ "))
        }),
        "work that finished out of sight was not reported as done\n{}",
        h.drawn()
    );
}

#[test]
fn the_rail_gives_up_the_clock_before_it_gives_up_what_is_owed() {
    // The bar used to lay out the clock first and fit the counts in beside it
    // afterwards, so a narrow terminal kept the time and hid the fact that an
    // agent was waiting for a human. That is backwards: `blocked` is the only
    // state waiting on a person, and it is the last thing on the screen.
    let claude = fake_agent("claude");
    let mut h = Harness::start_with_config(&format!(
        "shell = {:?}\n[notify]\nenabled = false\n",
        claude.display().to_string()
    ));
    assert!(h.wait_for(READY, START), "never started");
    assert!(
        h.wait_until(Duration::from_secs(15), agent_at_rest),
        "not detected as an agent\n{}",
        h.drawn()
    );

    h.send(b"printf 'Do you want to proceed?\\n> 1. Yes\\n  2. No\\n'\r");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .row(ROWS - 1, COLS)
            .contains("! 1")),
        "the rail did not count a blocked agent\n{}",
        h.drawn()
    );

    // Down the ladder. At every width the count is still there, it is always
    // the same distance from the right edge, and by the bottom the clock is
    // not there at all.
    // "One place to look" is not a fixed column -- the exits themselves shrink
    // to marks further down the ladder, and everything left of them moves when
    // they do. It is that nothing is ever laid out between what is owed and
    // the way out, so the count is always found in the same place relative to
    // the thing at the end of the bar.
    let anchored = |rail: &str| {
        rail.split("! 1")
            .nth(1)
            .and_then(|after| after.split('✕').next())
            .is_some_and(|between| between.trim().is_empty())
    };
    assert!(
        anchored(&h.row(ROWS - 1, COLS)),
        "not anchored to begin with"
    );
    let mut clock_dropped = false;
    for cols in [64u16, 52, 44, 36, 30, 24] {
        h.resize(cols);
        let rail = h.row(ROWS - 1, cols);
        assert!(
            rail.contains("! 1"),
            "at {cols} columns the rail stopped saying an agent was blocked\n{rail}"
        );
        assert!(
            anchored(&rail),
            "at {cols} columns something came between what is owed and the way out\n{rail}"
        );
        clock_dropped |= !rail.contains(':');
    }
    assert!(
        clock_dropped,
        "the clock survived every width, so nothing was actually given up"
    );
}

#[test]
fn a_chord_that_is_open_says_so_where_you_are_looking() {
    // The prefix indicator used to be smuggled through the status note and
    // drawn in the same dim as the clock. It is the most time-critical thing
    // the interface says — it is over in half a second — and it now displaces
    // where you are, because mid-chord nothing else on the bar matters.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf '\\033]2;Reading the vt100 grid\\007'\r");
    assert!(
        h.wait_until(Duration::from_secs(20), |h| h
            .row(ROWS - 1, COLS)
            .contains("vt100 grid")),
        "the rail never named where it was\n{}",
        h.drawn()
    );

    // The prefix alone, with no second key: the bar has to say it is waiting.
    h.send(&[0]);
    assert!(
        h.wait_until(Duration::from_secs(5), |h| {
            !h.row(ROWS - 1, COLS).contains("vt100 grid")
        }),
        "an open chord did not displace where you are\n{}",
        h.drawn()
    );

    // Escape closes it and puts the crumb back.
    h.send(b"\x1b");
    assert!(
        h.wait_until(Duration::from_secs(5), |h| h
            .row(ROWS - 1, COLS)
            .contains("vt100 grid")),
        "the bar did not come back after the chord\n{}",
        h.drawn()
    );
}

#[test]
fn the_rail_fits_every_width_it_is_given() {
    // The bar is the one surface that spans the whole terminal, so anything it
    // gets wrong about width it gets wrong across the screen: a run that
    // overshoots wraps onto the row above and corrupts a pane.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    let above = h.row(ROWS - 2, COLS);
    for cols in (16u16..=80).step_by(6) {
        h.resize(cols);
        let rail = h.row(ROWS - 1, cols);
        assert!(
            rail.chars().count() <= cols as usize,
            "at {cols} columns the rail was {} wide\n{rail}",
            rail.chars().count()
        );
        assert!(
            rail.contains('◆'),
            "at {cols} columns the rail said nothing at all\n{rail}"
        );
        assert!(
            rail.contains('✕'),
            "at {cols} columns there was no way out of the session\n{rail}"
        );
        // Wrapping would land here, on top of whatever the pane drew.
        assert!(
            !h.row(ROWS - 2, cols).contains('◆'),
            "at {cols} columns the rail wrapped onto the row above\n{}",
            h.drawn()
        );
    }
    h.resize(COLS);
    assert!(
        !above.contains('◆'),
        "the row above the rail held the rail before any of this"
    );
}

#[test]
fn a_chip_in_the_rail_is_still_a_way_to_get_there() {
    // The chips are only there when the nav is hidden, which is the one time
    // they are the only navigation. They have to work.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf '\\033]2;First thing\\007'\r");
    std::thread::sleep(Duration::from_secs(3));
    h.prefix(b"n");
    std::thread::sleep(Duration::from_millis(500));
    h.send(b"printf '\\033]2;Second thing\\007'\r");
    std::thread::sleep(Duration::from_secs(3));

    h.prefix(b"b");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            let r = h.row(ROWS - 1, COLS);
            r.contains("1 First") && r.contains("2 Second")
        }),
        "the rail did not become the list\n{}",
        h.drawn()
    );

    // Click the one that is not focused, and the bar says you are in it.
    let rail = h.row(ROWS - 1, COLS);
    let at = rail.find("1 First").expect("the first chip") as u16;
    h.click(at, ROWS - 1);
    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            let r = h.row(ROWS - 1, COLS);
            // The filled bar marks the chip you are in; it has to have moved.
            r.split('▊')
                .nth(1)
                .is_some_and(|after| after.starts_with("1 "))
        }),
        "clicking a chip did not go there\n{}",
        h.drawn()
    );
}

#[test]
fn the_corner_is_the_safe_way_out() {
    // The bottom-right is the cheapest target a pointer has: you can throw the
    // mouse at it and it cannot overshoot. Giving that to the thing that ends
    // every shell and every agent was backwards, and the two-click arming on
    // quit was compensating for it.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    let rail = h.row(ROWS - 1, COLS);
    let detach = rail.find("detach").expect("no detach in the rail");
    let quit = rail.find("quit").expect("no quit in the rail");
    assert!(
        detach > quit,
        "quit is outboard of detach, so overshooting the safe one hits the dangerous one\n{rail}"
    );
    assert!(
        rail.trim_end().ends_with("detach"),
        "something other than detach holds the corner\n{rail}"
    );
    // And they are not neighbours. A stray click between them should land on
    // ground, not on the other one.
    assert!(
        quit + "quit".len() + 2 <= detach,
        "quit and detach are adjacent\n{rail}"
    );
}

#[test]
fn the_rail_shows_what_the_nav_is_not_showing() {
    // One rule, not two modes. With the nav up the list is right there, so the
    // bar names where you are; with it hidden nothing else is showing the
    // session, so the bar becomes the list.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf '\\033]2;Reading the vt100 grid\\007'\r");
    assert!(
        h.wait_until(Duration::from_secs(20), |h| h
            .row(ROWS - 1, COLS)
            .contains("vt100 grid")),
        "the rail never named where it was\n{}",
        h.drawn()
    );
    let with_nav = h.row(ROWS - 1, COLS);
    assert!(
        with_nav.contains('▸'),
        "the rail did not say where you are as a path\n{with_nav}"
    );

    h.prefix(b"b");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .row(ROWS - 1, COLS)
            .contains('▊')),
        "hiding the nav did not put the list in the rail\n{}",
        h.drawn()
    );
    // And back again: the rule reads the same in both directions.
    h.prefix(b"b");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            let r = h.row(ROWS - 1, COLS);
            r.contains('▸') && !r.contains('▊')
        }),
        "showing the nav again left the list in the rail\n{}",
        h.drawn()
    );
}

#[test]
fn the_rail_counts_what_is_owed_and_nothing_else() {
    let claude = fake_agent("claude");
    let mut h = Harness::start_with_config(&format!(
        "shell = {:?}\n[notify]\nenabled = false\n",
        claude.display().to_string()
    ));
    assert!(h.wait_for(READY, START), "never started");

    let rail = |h: &Harness| h.rows().last().cloned().unwrap_or_default();

    // Nothing owed, nothing said. A pair of zeroes is not information.
    assert!(
        h.wait_until(Duration::from_secs(15), agent_at_rest),
        "not detected as an agent\n{}",
        h.drawn()
    );
    assert!(
        !rail(&h).contains("! "),
        "counted a block that had not happened\n{}",
        rail(&h)
    );

    // Blocked shows up, in the rail, with a count.
    h.send(b"printf 'Do you want to proceed?\\n> 1. Yes\\n  2. No\\n'\r");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| rail(h).contains("! 1")),
        "the rail did not count a blocked agent\n{}",
        h.drawn()
    );

    // And goes away again when it does.
    h.send(b"clear\r");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| !rail(h).contains("! 1")),
        "the count outlived the block\n{}",
        h.drawn()
    );
}

#[test]
fn an_agent_is_given_a_name_you_could_type() {
    let claude = fake_agent("claude");
    let mut h = Harness::start_with_config(&format!(
        "shell = {:?}\n[notify]\nenabled = false\n",
        claude.display().to_string()
    ));
    assert!(h.wait_for(READY, START), "never started");

    // Name the workspace first: an agent's name comes from the intent of the
    // workspace it is in, so `dirk agent send-keys reviewing-the-parser` will
    // name something you would recognise.
    h.send(b"printf '\\033]2;Reviewing the parser\\007'\r");

    // In the nav, not merely on screen. The pane echoes the command that set
    // the title and that appears at once, so waiting on it would race the
    // naming debounce -- and splitting before the name lands means it never
    // does, since a workspace holding two panes has no single intent.
    let nav_row = |h: &Harness| {
        h.rows().iter().position(|r| {
            r.chars()
                .take(34)
                .collect::<String>()
                .contains("Reviewing the parser")
        })
    };
    assert!(
        h.wait_until(Duration::from_secs(20), |h| nav_row(h).is_some()),
        "the workspace was never named\n{}",
        h.drawn()
    );

    // Two panes, so the workspace has a subtree worth opening.
    h.prefix(b"|");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| nav_row(h).is_some()),
        "the workspace row went missing after the split\n{}",
        h.drawn()
    );
    let row = nav_row(&h).expect("the workspace row");
    h.click(4, row as u16);

    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            h.rows().iter().any(|r| r.contains("reviewing-the-parser"))
        }),
        "no agent name in the expanded workspace\n{}",
        h.drawn()
    );
    // Uniqueness is a property of the names, not of what the nav happens to
    // show: a pane that has published a title displays that instead, which is
    // more useful in that row. `name::unique` is tested directly.
}

#[test]
fn a_label_is_arranged_by_its_template() {
    // `{n}` rather than `{branch}`: a CI checkout can be on a detached head,
    // where the branch token is absent -- which is its own test below, not
    // this one's business.
    let mut h = Harness::start_with_config(
        "[naming.templates]\nworkspace = \"#{n} {intent}\"\n[notify]\nenabled = false\n",
    );
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf '\\033]2;Reading the vt100 grid\\007'\r");
    assert!(
        h.wait_until(Duration::from_secs(20), |h| {
            // The counter, which is what the template arranged, and the end of
            // the intent, which is what shortening keeps. Not `#1 Reading`:
            // that asserts the intent was not shortened, which is a different
            // test and no longer true at this width.
            h.rows().iter().any(|r| {
                let row: String = r.chars().take(34).collect();
                row.contains("#1 ") && row.contains("vt100 grid")
            })
        }),
        "the label was not arranged by the template\n{}",
        h.drawn()
    );

    // And it keeps renaming. The label is the template's output and the intent
    // is what it came from; comparing the one against the other made every
    // named workspace look hand-written the moment it was named, so a
    // non-default template froze after exactly one rename.
    h.send(b"printf '\\033]2;Something else entirely\\007'\r");
    assert!(
        h.wait_until(Duration::from_secs(25), |h| {
            h.rows().iter().any(|r| {
                let row: String = r.chars().take(34).collect();
                row.contains("#1 ") && row.contains("entirely")
            })
        }),
        "a templated label froze after one rename\n{}",
        h.drawn()
    );
}

#[test]
fn a_program_that_titles_itself_does_not_become_a_workspace_name() {
    // Shells and editors publish their own name as a title. Without the ignore
    // list they arrive as intents: a pane running `nvim` becomes a workspace
    // called "nvim".
    let mut h = Harness::start_with_config("[notify]\nenabled = false\n");
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf '\\033]2;lazygit\\007'\r");
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !h.rows().iter().any(
            |r| r.chars().take(34).collect::<String>().contains("lazygit")
                && !r.contains("  lazygit")
        ),
        "a program name became a workspace label\n{}",
        h.drawn()
    );
}

#[test]
fn two_intents_sharing_a_verb_are_still_told_apart() {
    // Elision used to cut the tail. The verb is shared with every other row and
    // the noun is the whole of what tells them apart, so two workspaces both
    // read "Building the …" and the list stopped being a way to choose between
    // them. Rows are the place to assert it: they sit adjacent, which is the
    // only situation in which two labels have to differ.
    let mut h = Harness::start_with_config("[notify]\nenabled = false\n");
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf '\\033]2;Building the mux core\\007'\r");
    std::thread::sleep(Duration::from_secs(3));
    h.prefix(b"n");
    std::thread::sleep(Duration::from_millis(400));
    h.send(b"printf '\\033]2;Building the release notes\\007'\r");

    // The nav is the sidebar's width, so both labels are shortened there too.
    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            let nav: Vec<String> = h
                .rows()
                .iter()
                .map(|r| r.chars().take(34).collect())
                .collect();
            nav.iter().any(|r| r.contains("mux core")) && nav.iter().any(|r| r.contains("release"))
        }),
        "two workspaces sharing a verb were not told apart\n{}",
        h.drawn()
    );
}

#[test]
fn the_second_source_enabled_without_a_key_changes_nothing() {
    // Every failure of the second source has to leave naming exactly where it
    // was without it: no key, no curl, a timeout, a bad answer, all the same.
    // A naming source that can break naming is worse than no naming source.
    let claude = fake_agent("claude");
    let mut h = Harness::start_with_config(&format!(
        r#"shell = {:?}
[notify]
enabled = false
[naming.sources.llm]
enabled = true
api_key_env = "DIRK_A_KEY_THAT_IS_NOT_SET"
interval_ms = 0
"#,
        claude.display().to_string()
    ));
    assert!(h.wait_for(READY, START), "never started");

    // The title source still works, and the workspace is still named by it.
    h.send(b"printf '\\033]2;Still named by the title\\007'\r");
    assert!(
        h.wait_until(Duration::from_secs(20), |h| {
            // The tail of the title rather than its head: what this test is
            // about is that the title named the workspace and the second
            // source did not, and either end proves that.
            h.rows().iter().any(|r| {
                r.chars()
                    .take(34)
                    .collect::<String>()
                    .contains("by the title")
            })
        }),
        "naming stopped working with the second source enabled\n{}",
        h.drawn()
    );
}

#[test]
fn the_nav_hides_and_comes_back() {
    // Guarded because this key moved: `d` used to hide the nav and now detaches,
    // and a test that presses the wrong one and passes anyway is how that goes
    // unnoticed.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");
    assert!(h.find("spaces").is_some(), "the nav was never there");

    h.prefix(b"b");
    assert!(
        h.wait_until(Duration::from_secs(5), |s| s.find("spaces").is_none()),
        "the nav did not hide\n{}",
        h.drawn()
    );

    h.prefix(b"b");
    assert!(
        h.wait_for("spaces", Duration::from_secs(5)),
        "the nav did not come back\n{}",
        h.drawn()
    );
}

// ── The marks, and the column they have to fit in ───────────────────────

#[test]
fn the_ascii_set_draws_a_nav_with_nothing_outside_ascii_in_it() {
    // The set exists for terminals and fonts that cannot draw the default one.
    // A set that reaches for `▾` in one forgotten place is a set that does not
    // solve the problem it was added for.
    let mut h = Harness::start_with_config("[nav]\nglyphs = \"ascii\"\n");
    assert!(h.wait_for(READY, START), "never started");

    for row in h.rows() {
        assert!(
            row.is_ascii(),
            "the ascii set drew something that is not ascii:\n{}\n{}",
            row,
            h.drawn()
        );
    }
}

#[test]
fn a_name_two_columns_wide_does_not_push_the_age_off_the_row() {
    // The nav reserves the age before it writes the name. Counting characters
    // where the terminal counts columns is how a title an agent wrote in
    // Japanese -- or with one emoji in it -- moves every column after it.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    // "\u{8aad}\u{307f}\u{8fbc}\u{307f}\u{4e2d}\u{3067}\u{3059}" -- fourteen columns of seven characters.
    h.send(
        "printf '\x1b]2;\u{8aad}\u{307f}\u{8fbc}\u{307f}\u{4e2d}\u{3067}\u{3059}\x07'\r".as_bytes(),
    );
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .rows()
            .iter()
            .any(|r| r.contains('読'))),
        "the wide name never arrived\n{}",
        h.drawn()
    );

    // The nav is the left column and the pane starts after it. A row that
    // overran would have written into the pane's first column.
    let row = h
        .rows()
        .into_iter()
        .find(|r| r.contains('読'))
        .expect("the row is there");
    assert!(
        row.contains("now") || row.contains('s') || row.contains('m'),
        "the age was pushed off the row by a wide name:\n{row}"
    );
}

#[test]
fn a_folded_project_still_says_something_is_waiting_inside_it() {
    // Folding a project to make twenty of them fit must not also hide the one
    // agent that is blocked. That is the thing the column exists to show.
    let claude = fake_agent("claude");
    let mut h =
        Harness::start_with_config(&format!("shell = {:?}\n", claude.display().to_string()));
    assert!(h.wait_for(READY, START), "never started");
    assert!(
        h.wait_until(Duration::from_secs(15), agent_at_rest),
        "not detected as an agent\n{}",
        h.drawn()
    );

    h.send(b"printf 'Do you want to proceed?\\n> 1. Yes\\n  2. No\\n'\r");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .rows()
            .iter()
            .any(|r| r.trim_start().starts_with("! "))),
        "never read as blocked\n{}",
        h.drawn()
    );

    // Fold it. The project row is the one with the disclosure mark on it.
    let (row, col) = h.find("▾").expect("a project to fold");
    h.click(col as u16, row as u16);

    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            h.rows()
                .iter()
                .any(|r| r.trim_start().starts_with("▸ ") && r.contains('!'))
        }),
        "a folded project said nothing about the agent waiting inside it\n{}",
        h.drawn()
    );
}

#[test]
fn short_rows_drop_the_intent_and_keep_the_identity() {
    // Two lines a workspace is right at five and wrong at forty. What goes is
    // the caption -- the identity line is the row itself, and dropping that
    // would leave a list of things doing something with no way to say which.
    let mut h = Harness::start_with_config("[nav]\nrows = \"short\"\n");
    assert!(h.wait_for(READY, START), "never started");
    std::thread::sleep(Duration::from_secs(2));

    let rows = h.rows();
    assert!(
        rows.iter().any(|r| r.contains("dirk")),
        "the project went with the caption\n{}",
        h.drawn()
    );
    // One row per space: the identity line, and then straight to the footer
    // that offers another. A caption would sit between the two.
    let from = rows
        .iter()
        .position(|line| line.trim_start().starts_with("spaces"))
        .expect("the spaces heading");
    let at = rows
        .iter()
        .enumerate()
        .skip(from)
        .find_map(|(r, line)| {
            let i = line.find("1 ")?;
            line[..i].trim().is_empty().then_some(r)
        })
        .expect("the space's identity line");
    assert!(
        rows[at + 1].contains("workspace"),
        "a second line survived the short form\n{}",
        h.drawn()
    );
}

// ── Boards that report ──────────────────────────────────────────────────

#[test]
fn a_board_says_what_it_has_to_say_without_being_opened() {
    // The difference between a link and an instrument. A dashboard you have to
    // open to find out whether it matters is a link with extra steps.
    let mut h = Harness::start_with_config(
        "[[board]]\n\
         name    = \"git\"\n\
         command = [\"sh\"]\n\
         status  = { run = [\"printf\", \"3up 2dot\"], every = \"2s\" }\n",
    );
    assert!(h.wait_for("boards", START), "never started\n{}", h.drawn());
    assert!(
        h.wait_for("3up 2dot", Duration::from_secs(15)),
        "the board never reported\n{}",
        h.drawn()
    );
}

#[test]
fn a_status_command_that_cannot_run_says_so_rather_than_nothing() {
    // A command that fails is a configuration problem. Showing a dash says the
    // board was asked and could not answer, which is different from a board
    // that was never asked at all.
    let mut h = Harness::start_with_config(
        "[[board]]\n\
         name    = \"broken\"\n\
         command = [\"sh\"]\n\
         status  = { run = [\"definitely-not-a-program\"], every = \"2s\" }\n",
    );
    assert!(h.wait_for("boards", START), "never started");
    assert!(
        h.wait_for("—", Duration::from_secs(15)),
        "a board that could not answer said nothing at all\n{}",
        h.drawn()
    );
}

#[test]
fn the_old_word_for_a_board_still_parses() {
    // `[[layout]]` is what these were called first. Somebody's configuration
    // should not stop working because the interface learned a better word.
    let mut h =
        Harness::start_with_config("[[layout]]\nname = \"still-here\"\ncommand = [\"sh\"]\n");
    assert!(
        h.wait_for("still-here", START),
        "an existing config stopped working\n{}",
        h.drawn()
    );
}

// ── Reading what has gone past ──────────────────────────────────────────

#[test]
fn output_that_left_the_screen_can_be_read_again() {
    // vt100 was keeping scrollback all along and dirk never showed any of it.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    // More lines than the screen has, so the first is gone from it.
    h.send(b"for i in $(seq 1 60); do printf 'zzLINE%s\\n' $i; done\r");
    assert!(
        h.wait_for("zzLINE60", Duration::from_secs(10)),
        "the output never arrived\n{}",
        h.drawn()
    );
    assert!(
        h.find("zzLINE1 ").is_none(),
        "the screen is not full, so there is nothing to scroll back to\n{}",
        h.drawn()
    );

    h.prefix(b"[");
    // Page up rather than `k`: `k` moves the reading cursor and only scrolls
    // once it reaches the top, which is right for selecting and slow for
    // looking.
    for _ in 0..4 {
        h.send(b"\x1b[5~");
    }
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzLINE10").is_some()),
        "scrolling back showed nothing that had left the screen\n{}",
        h.drawn()
    );

    // And the bar says you are not looking at the live screen, which a pane
    // that has simply stopped changing looks exactly like. It says it with a
    // mark and a depth, beside what is owed rather than beside the clock:
    // being read from the past is a mode, and a mode drawn as ambient text is
    // a mode nobody notices.
    assert!(
        h.rows().last().is_some_and(|r| {
            r.split('↑')
                .nth(1)
                .is_some_and(|d| d.starts_with(|c: char| c.is_ascii_digit()) && !d.starts_with('0'))
        }),
        "nothing said the pane was being read from the past\n{}",
        h.drawn()
    );

    // Leaving puts it back.
    h.send(b"\x1b");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzLINE60").is_some()),
        "leaving did not return to the live screen\n{}",
        h.drawn()
    );
}

#[test]
fn typing_brings_a_scrolled_pane_back_to_the_live_screen() {
    // Sending a keystroke to a program whose output you cannot see is the kind
    // of thing you find out about afterwards.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");
    h.send(b"for i in $(seq 1 60); do printf 'zzL%s\\n' $i; done\r");
    assert!(h.wait_for("zzL60", Duration::from_secs(10)), "no output");

    h.prefix(b"[");
    for _ in 0..4 {
        h.send(b"\x1b[5~");
    }
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzL10").is_some()),
        "did not scroll\n{}",
        h.drawn()
    );

    // `q` rather than escape: a lone escape byte followed immediately by more
    // input parses as alt-something, which is a property of terminals and not
    // of dirk.
    h.send(b"q");
    std::thread::sleep(Duration::from_millis(300));
    h.send(b"printf 'zzAFTER'\r");
    assert!(
        h.wait_for("zzAFTER", Duration::from_secs(10)),
        "what was typed went somewhere invisible\n{}",
        h.drawn()
    );
}

#[test]
fn a_selection_is_taken_and_the_terminal_is_asked_to_hold_it() {
    // The escape sequence is the half that crosses ssh, so it is the half worth
    // asserting: the terminal at the other end is the one with the clipboard.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");
    h.send(b"printf 'zzTAKEME\\n'\r");
    assert!(h.wait_for("zzTAKEME", Duration::from_secs(10)), "no output");

    let (row, col) = h.find("zzTAKEME").expect("the text is on screen");
    h.drag(col as u16, row as u16, col as u16 + 7, row as u16);

    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .rows()
            .last()
            .is_some_and(|r| r.contains("copied"))),
        "nothing was copied\n{}",
        h.drawn()
    );
}

// ── Finding it ──────────────────────────────────────────────────────────

#[test]
fn a_line_that_has_left_the_screen_can_still_be_found() {
    // Finding the error a build printed four minutes ago used to mean scrolling
    // and reading.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.send(b"printf 'zzNEEDLE here\\n'\r");
    assert!(h.wait_for("zzNEEDLE", Duration::from_secs(10)), "no output");
    // Push it off the screen.
    h.send(b"for i in $(seq 1 60); do printf 'filler%s\\n' $i; done\r");
    assert!(h.wait_for("filler60", Duration::from_secs(10)), "no filler");
    assert!(
        h.find("zzNEEDLE").is_none(),
        "it never left the screen, so there is nothing to find\n{}",
        h.drawn()
    );

    h.prefix(b"/");
    h.send(b"zzNEEDLE");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzNEEDLE").is_some()),
        "the search found nothing\n{}",
        h.drawn()
    );
    // And says how many, which an incremental search with no count leaves you
    // typing at hopefully.
    assert!(
        h.rows().iter().any(|r| r.contains(" of ")),
        "no count beside the query\n{}",
        h.drawn()
    );
}

#[test]
fn leaving_a_search_puts_you_back_where_you_were() {
    // A search you abandoned should cost you nothing, including your place.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");
    h.send(b"printf 'zzOLD\\n'\r");
    assert!(h.wait_for("zzOLD", Duration::from_secs(10)), "no output");
    h.send(b"for i in $(seq 1 60); do printf 'pad%s\\n' $i; done\r");
    assert!(h.wait_for("pad60", Duration::from_secs(10)), "no filler");

    h.prefix(b"/");
    h.send(b"zzOLD");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzOLD").is_some()),
        "the search found nothing\n{}",
        h.drawn()
    );

    h.send(b"\x1b");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("pad60").is_some()),
        "leaving the search did not put the pane back\n{}",
        h.drawn()
    );
}

// ── One pane, and putting them back ─────────────────────────────────────

#[test]
fn zoom_gives_one_pane_the_whole_space_and_gives_it_back() {
    // A view rather than a change: leaving has to put every pane back exactly
    // where it was, with what is running in it untouched.
    let mut h = Harness::start_with_config("[nav]\nglyphs = \"ascii\"\n");
    assert!(h.wait_for(READY, START), "never started");

    h.prefix(b"b"); // the nav out of the way, so the panes are wide
    h.prefix(b"|");
    h.send(b"printf 'zzRIGHT'\r");
    h.prefix(b";");
    h.send(b"printf 'zzLEFT'\r");
    assert!(
        h.wait_for("zzRIGHT", Duration::from_secs(10)),
        "no right pane"
    );
    assert!(
        h.wait_for("zzLEFT", Duration::from_secs(10)),
        "no left pane"
    );

    h.prefix(b"z");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzRIGHT").is_none()),
        "zoom did not hide the other pane\n{}",
        h.drawn()
    );

    h.prefix(b"z");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h.find("zzRIGHT").is_some()),
        "leaving zoom did not put the other pane back\n{}",
        h.drawn()
    );
    assert!(
        h.find("zzLEFT").is_some(),
        "the zoomed pane lost what was in it\n{}",
        h.drawn()
    );
}

#[test]
fn a_pane_can_be_moved_past_its_neighbour() {
    // Neither program is restarted, so both markers survive the move -- they
    // are just on the other side of the screen.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");
    h.prefix(b"b");
    h.prefix(b"|");
    h.send(b"printf 'zzB'\r");
    h.prefix(b";");
    h.send(b"printf 'zzA'\r");
    assert!(h.wait_for("zzB", Duration::from_secs(10)), "no second pane");
    assert!(h.wait_for("zzA", Duration::from_secs(10)), "no first pane");

    let before = h.find("zzA").expect("on screen").1;
    h.prefix(b"}");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .find("zzA")
            .is_some_and(|(_, col)| col != before)),
        "the pane did not move\n{}",
        h.drawn()
    );
    assert!(
        h.find("zzB").is_some(),
        "the pane it moved past lost what was in it\n{}",
        h.drawn()
    );
}

// ── Everything dirk can do ──────────────────────────────────────────────

#[test]
fn the_palette_lists_what_dirk_can_do_and_what_to_press_instead() {
    // The answer to "how do I", and the thing that takes the pressure off
    // binding everything.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.prefix(b"p");
    assert!(
        h.wait_for("Open a project", Duration::from_secs(10)),
        "the palette did not open\n{}",
        h.drawn()
    );
    // The key beside it, because a palette that does not teach you the key is
    // one you keep coming back to.
    assert!(
        h.rows().iter().any(|r| r.contains("o  Open a project")),
        "no key beside the action\n{}",
        h.drawn()
    );

    // What cannot be done now is shown with the reason rather than hidden.
    assert!(
        h.rows()
            .iter()
            .any(|r| r.contains("Zoom") && r.contains("only one pane")),
        "an unavailable action was hidden rather than explained\n{}",
        h.drawn()
    );

    // Typing filters, out of order and by subsequence.
    h.send(b"opro");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| {
            h.rows().iter().any(|r| r.contains("Open a project"))
                && !h.rows().iter().any(|r| r.contains("Split into rows"))
        }),
        "typing did not filter\n{}",
        h.drawn()
    );
}

#[test]
fn the_palette_is_a_way_to_go_somewhere_as_well() {
    // Boards and spaces are places as much as the actions are things.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");
    h.prefix(b"p");
    assert!(
        h.wait_for("Open a project", Duration::from_secs(10)),
        "no palette"
    );
    // Typed for, because the actions come first and there are more of them than
    // there are rows.
    h.send(b"goto");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .rows()
            .iter()
            .any(|r| r.contains("Go to "))),
        "nowhere to go from the palette\n{}",
        h.drawn()
    );
}

#[test]
fn a_key_can_be_moved_and_the_old_one_stops_working() {
    // A rebind that leaves the old key working is one that looks like it did
    // not take.
    let mut h = Harness::start_with_config("[keys]\n\"nav.toggle\" = \"H\"\n");
    assert!(h.wait_for(READY, START), "never started");
    assert!(h.find("spaces").is_some(), "the nav was never there");

    h.prefix(b"b");
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        h.find("spaces").is_some(),
        "the old key still hides the nav\n{}",
        h.drawn()
    );

    h.prefix(b"H");
    assert!(
        h.wait_until(Duration::from_secs(5), |h| h.find("spaces").is_none()),
        "the new key does not hide the nav\n{}",
        h.drawn()
    );
}

#[test]
fn a_binding_for_something_that_does_not_exist_is_reported() {
    // A line in a file that looks like it works is worse than one that is
    // rejected.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .args(["--keys"])
        .env("XDG_CONFIG_HOME", {
            let dir = std::env::temp_dir().join("dirk-badkeys").join(format!(
                "{}-{}",
                std::process::id(),
                next_config_id()
            ));
            std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
            std::fs::write(
                dir.join("dirk").join("config.toml"),
                "[keys]\n\"nav.togle\" = \"H\"\n",
            )
            .expect("config");
            dir
        })
        .output()
        .expect("run dirk");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("nav.togle"),
        "a binding for nothing was accepted in silence: {said}"
    );
}

#[test]
fn a_note_never_costs_the_bar_its_way_out() {
    // The status note used to be drawn at whatever length it happened to be,
    // whatever room there was. On a narrow terminal nothing else could then
    // fit, every step of the ladder failed, and the bar fell back to drawing
    // the mark alone -- so copy mode on a small screen lost the way out, the
    // clock, and anything that was owed, at the moment it was hardest to
    // guess at.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started");

    h.prefix(b"[");
    assert!(
        h.wait_until(Duration::from_secs(10), |h| h
            .row(ROWS - 1, COLS)
            .contains("copy")),
        "copy mode said nothing in the rail\n{}",
        h.drawn()
    );

    for cols in [72u16, 64, 56, 48, 40, 32, 24] {
        h.resize(cols);
        let rail = h.row(ROWS - 1, cols);
        assert!(
            rail.contains('✕'),
            "at {cols} columns a note took the way out off the bar\n{rail}"
        );
        assert!(
            rail.chars().count() <= cols as usize,
            "at {cols} columns the note overran the terminal\n{rail}"
        );
    }
}

/// How many pane rules are on the screen.
///
/// Counted as runs rather than characters: two panes side by side put both of
/// their rules on one row, and splitting makes each of them shorter — so the
/// total length goes *down* while the number of rules goes up.
fn rules(h: &Harness) -> usize {
    h.rows()
        .iter()
        .map(|row| {
            row.split(|c| c != '─')
                .filter(|run| run.chars().count() >= 5)
                .count()
        })
        .sum()
}

#[test]
fn every_pane_can_carry_a_rule_that_says_which_one_has_the_keyboard() {
    // Two shells side by side have no visible boundary: the grids abut, and
    // nothing says where one ends or which has the keyboard. A rule rather than
    // a border — three sides of a box only repeat what the neighbour's own edge
    // already says.
    let mut h = Harness::start_with_config("[ui]\npane_rules = \"always\"\n");
    assert!(h.wait_for(READY, START), "never started\n{}", h.drawn());

    assert_eq!(rules(&h), 1, "one pane drew no rule\n{}", h.drawn());
    h.send(b"\x00|");
    assert!(
        h.wait_until(START, |h| rules(h) == 2),
        "a split of two shells drew one rule between them\n{}",
        h.drawn()
    );
    drop(h);

    // And under the default nothing changes: a rule is for a pane with
    // something to say, which is a board's panel.
    let mut plain = Harness::start();
    assert!(
        plain.wait_for(READY, START),
        "never started\n{}",
        plain.drawn()
    );
    assert_eq!(
        rules(&plain),
        0,
        "an unlabelled pane drew a rule\n{}",
        plain.drawn()
    );
    plain.send(b"\x00|");
    std::thread::sleep(std::time::Duration::from_millis(800));
    assert_eq!(
        rules(&plain),
        0,
        "splitting drew a rule under the default\n{}",
        plain.drawn()
    );
}

/// Which column the pane focus mark is in, so a click can be seen to have moved
/// it. Boards are not usable for this: one whose program is not installed is
/// dropped at startup, so which rows exist depends on the machine.
fn focus_mark(h: &Harness) -> Option<usize> {
    // Columns, not bytes: a rule is made of `─`, which is three bytes and one
    // column, so a byte offset says the edge moved three times as far as it did.
    h.rows()
        .iter()
        .find_map(|row| row.chars().position(|c| c == '\u{258A}'))
}

#[test]
fn the_mouse_can_be_left_to_the_terminal_that_owns_it() {
    // All or nothing. dirk captures so it can decide per pane whether the
    // program inside wanted the click; declining hands the outer terminal its
    // own selection back, and everything a click reaches has a key.
    let ruled = "[ui]\npane_rules = \"always\"\n";
    let mut off = Harness::start_with_config(&format!("{ruled}mouse = false\n"));
    assert!(off.wait_for(READY, START), "never started\n{}", off.drawn());
    off.send(b"\x00|");
    assert!(
        off.wait_until(START, |h| rules(h) == 2),
        "the split never drew\n{}",
        off.drawn()
    );
    let was = focus_mark(&off).expect("a focused pane");

    // A click into the other pane does nothing. Dropped rather than merely not
    // asked for: the setting has to hold when something else turned reporting
    // on.
    off.click(40, 3);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        focus_mark(&off),
        Some(was),
        "a click reached dirk after the mouse was turned off\n{}",
        off.drawn()
    );

    // And the nav is still reachable, which is the promise that makes turning
    // it off reasonable.
    off.send(b"\x00w");
    assert!(
        off.wait_until(START, |h| h.drawn().contains('\u{25B8}')),
        "the nav could not be reached without a mouse\n{}",
        off.drawn()
    );
    drop(off);

    // The same click, with the mouse on, moves the focus.
    let mut on = Harness::start_with_config(ruled);
    assert!(on.wait_for(READY, START), "never started\n{}", on.drawn());
    on.send(b"\x00|");
    assert!(
        on.wait_until(START, |h| rules(h) == 2),
        "the split never drew\n{}",
        on.drawn()
    );
    let was = focus_mark(&on).expect("a focused pane");
    on.click(40, 3);
    assert!(
        on.wait_until(START, |h| focus_mark(h) != Some(was)),
        "a click did nothing with the mouse on\n{}",
        on.drawn()
    );
}

#[test]
fn dirk_says_what_the_window_it_is_in_should_be_called() {
    // dirk emulates the terminals in its panes, so a title written inside one
    // stops at dirk — and it wrote none of its own, leaving the window holding
    // fourteen panes labelled whatever it was called before dirk started.
    let mut h = Harness::start_with_config("[ui]\nwindow_title = \"zzT {workspace}\"\n");
    assert!(h.wait_for(READY, START), "never started\n{}", h.drawn());
    assert!(
        h.wait_until(START, |h| h.title().is_some_and(|t| t.starts_with("zzT "))),
        "dirk did not name the window it is in: {:?}",
        h.title()
    );
    drop(h);

    // Empty leaves the terminal's own title alone, which is what dirk did
    // before it wrote one at all.
    let mut quiet = Harness::start_with_config("[ui]\nwindow_title = \"\"\n");
    assert!(
        quiet.wait_for(READY, START),
        "never started\n{}",
        quiet.drawn()
    );
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(quiet.title(), None, "an empty template still wrote a title");
}

#[test]
fn copy_mode_moves_by_words_the_way_every_editor_does() {
    // Selecting an identifier out of a stack trace meant holding `l`. This is
    // the gap people actually feel, because every editor they use has `w`.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started\n{}", h.drawn());
    h.send(b"printf 'zzalpha.beta  gamma\\n'\r");
    assert!(
        h.wait_for("zzalpha.beta", START),
        "the line never printed\n{}",
        h.drawn()
    );

    // Into copy mode, to the start of that line, then select one word.
    h.send(b"\x00[");
    assert!(
        h.wait_until(START, |h| h.drawn().contains("y copy")),
        "copy mode did not open\n{}",
        h.drawn()
    );
    // Up to the printed line, to its start, then select one word: `zzalpha`
    // stops at the dot, which is what makes `w` useful in code.
    h.send(b"k0vwy");
    assert!(
        h.wait_until(START, |h| !h.drawn().contains("y copy")),
        "copying did not leave copy mode\n{}",
        h.drawn()
    );
}

#[test]
fn searching_inside_copy_mode_stays_in_the_pane_you_are_reading() {
    // `/` outside copy mode searches every pane, which is the right default and
    // better than what tmux does. It is not what you want once you are looking
    // at one pane's scrollback: the results take you away, and the answer
    // arrives as a list of places rather than as a cursor further up.
    let mut h = Harness::start();
    assert!(h.wait_for(READY, START), "never started\n{}", h.drawn());
    h.send(b"printf 'zzone\\nzztwo\\nzzthree\\n'\r");
    assert!(
        h.wait_for("zzthree", START),
        "nothing printed\n{}",
        h.drawn()
    );

    h.send(b"\x00[");
    assert!(
        h.wait_until(START, |h| h.drawn().contains("y copy")),
        "copy mode did not open\n{}",
        h.drawn()
    );

    // The query is echoed as it is typed, and every key is part of it — `y`
    // here is a letter, not the copy command.
    h.send(b"/zzty");
    assert!(
        h.wait_until(START, |h| h.drawn().contains("/zzty")),
        "the query was not echoed\n{}",
        h.drawn()
    );
    // Backspace takes it back, and Enter goes there.
    h.send(b"\x7f\r");
    assert!(
        h.wait_until(START, |h| h.drawn().contains("n next")),
        "the search did not land\n{}",
        h.drawn()
    );

    // Escape clears the search before it leaves, so the first press takes the
    // search off and the second leaves.
    h.send(b"\x1b");
    assert!(
        h.wait_until(START, |h| h.drawn().contains("y copy")
            && !h.drawn().contains("n next")),
        "escape did not clear the search first\n{}",
        h.drawn()
    );
    h.send(b"\x1b");
    assert!(
        h.wait_until(START, |h| !h.drawn().contains("y copy")),
        "escape did not leave copy mode\n{}",
        h.drawn()
    );
}

#[test]
fn a_split_can_be_resized_from_the_keyboard() {
    // The sidebar divider drags and the splits did not, from either input. A
    // layout you cannot adjust without reaching for a mouse is one people leave
    // wrong.
    let mut h = Harness::start_with_config("[ui]\npane_rules = \"always\"\n");
    assert!(h.wait_for(READY, START), "never started\n{}", h.drawn());
    h.send(b"\x00|");
    assert!(
        h.wait_until(START, |h| rules(h) == 2),
        "the split never drew\n{}",
        h.drawn()
    );
    let edge = |h: &Harness| focus_mark(h).expect("a focused pane");
    let before = edge(&h);

    // A count, so `10h` is one gesture rather than ten presses.
    h.send(b"\x00R10h");
    assert!(
        h.wait_until(START, |h| edge(h) + 10 == before),
        "the edge did not move ten columns: {} then {:?}\n{}",
        before,
        focus_mark(&h),
        h.drawn()
    );

    // Escape leaves, and the keys go back to the pane.
    h.send(b"\x1b");
    assert!(
        h.wait_until(START, |h| !h.drawn().contains("esc done")),
        "resize mode did not end\n{}",
        h.drawn()
    );
    let after = edge(&h);
    h.send(b"hhhh");
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        edge(&h),
        after,
        "keys still moved the edge after leaving\n{}",
        h.drawn()
    );
}

#[test]
fn a_chord_can_reach_an_action_with_no_prefix_at_all() {
    // The prefix form is the one somebody reading the manual finds; the direct
    // chord is the one their hands learn. Wanting both is the point.
    let mut h = Harness::start_with_config(
        "[keys]\n\"pane.split-cols\" = [\"prefix+v\", \"ctrl+alt+d\"]\n\
         [ui]\npane_rules = \"always\"\n",
    );
    assert!(h.wait_for(READY, START), "never started\n{}", h.drawn());
    assert_eq!(rules(&h), 1, "one pane to start with\n{}", h.drawn());

    // The chord, with nothing before it. `ctrl+alt+d` is ESC then ctrl-D.
    h.send(b"\x1b\x04");
    assert!(
        h.wait_until(START, |h| rules(h) == 2),
        "the chord did not reach the action\n{}",
        h.drawn()
    );

    // And the prefix form still works, because an action can have both.
    h.send(b"\x00v");
    assert!(
        h.wait_until(START, |h| rules(h) == 3),
        "the prefix form stopped working\n{}",
        h.drawn()
    );
}

#[test]
fn a_binding_that_could_never_work_is_said_out_loud() {
    // dirk cannot detect a chord the desktop ate, and the person choosing can.
    // Silence there is a binding that appears not to have worked with nothing
    // to say why.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_dirk"))
        .arg("--keys")
        .env("XDG_CONFIG_HOME", {
            let dir = std::env::temp_dir().join("dirk-badchord").join(format!(
                "{}-{}",
                std::process::id(),
                next_config_id()
            ));
            std::fs::create_dir_all(dir.join("dirk")).expect("config dir");
            std::fs::write(
                dir.join("dirk").join("config.toml"),
                "[keys]\n\"pane.zoom\" = \"ctrl+alt+t\"\n\"pane.close\" = \"ctrl+j\"\n\
                 \"nav.focus\" = \"ctrl+bogus\"\n\"no.such\" = \"x\"\n",
            )
            .expect("config");
            dir
        })
        .output()
        .expect("run dirk");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("terminal launcher"),
        "a taken chord passed: {said}"
    );
    assert!(
        said.contains("programs already use"),
        "ctrl+j passed: {said}"
    );
    assert!(
        said.contains("not a key dirk can read"),
        "nonsense passed: {said}"
    );
    assert!(
        said.contains("no action of that name"),
        "a typo passed: {said}"
    );
}
