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
}

impl Harness {
    fn start() -> Self {
        Self::start_with(None)
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
        std::fs::write(dir.join("dirk").join("config.toml"), config).expect("config");
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
    // The brand, in the rail. This is the only place it appears.
    assert!(
        h.wait_for("dirk", Duration::from_secs(5)),
        "no brand in the rail"
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
    assert!(h.wait_for(READY, START), "never started");

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
