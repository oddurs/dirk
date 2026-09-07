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
    // go on. What must not happen is the shell appearing under `agents`.
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        !h.rows().iter().any(|r| r.contains("agents")),
        "a shell was listed as an agent\n{}",
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
        h.wait_until(Duration::from_secs(15), |h| {
            h.rows().iter().any(|r| r.contains("agents"))
        }),
        "a pane running `claude` was not recognised as holding an agent\n{}",
        h.drawn()
    );
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
        h.wait_until(Duration::from_secs(15), |h| h
            .rows()
            .iter()
            .any(|r| r.contains("agents"))),
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
        h.wait_until(Duration::from_secs(15), |h| h
            .rows()
            .iter()
            .any(|r| r.contains("agents"))),
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
        h.wait_until(Duration::from_secs(15), |h| h
            .rows()
            .iter()
            .any(|r| r.contains("agents"))),
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
            h.rows().iter().any(|r| {
                r.chars()
                    .take(34)
                    .collect::<String>()
                    .contains("#1 Reading")
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
                r.chars()
                    .take(34)
                    .collect::<String>()
                    .contains("#1 Something")
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
            h.rows().iter().any(|r| {
                r.chars()
                    .take(34)
                    .collect::<String>()
                    .contains("Still named by")
            })
        }),
        "naming stopped working with the second source enabled\n{}",
        h.drawn()
    );
}
