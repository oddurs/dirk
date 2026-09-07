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

//! `cargo run --example shot` — what dirk actually paints, as text.
//!
//! Spawns the real binary on a pty, feeds its output through the same terminal
//! parser dirk uses for its own panes, and prints the resulting screen. Useful
//! for looking at the chrome without being inside it.
//!
//! `--html` prints the same screen with its colours, as spans. That is what
//! the website shows: a terminal render that is still text — selectable,
//! searchable, a few kilobytes, and correct by construction, because it came
//! out of the program rather than out of a screenshot of it.

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let (rows, cols) = (26u16, 92u16);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/debug/dirk"));
    // One process, and nothing kept. Attaching would join whatever session is
    // already running, so a shot would carry that session's accumulated
    // workspaces and no two runs would agree -- which for something committed
    // and diffed is the difference between a render and a photograph of a
    // moment.
    cmd.arg("--no-session");
    cmd.cwd(env!("CARGO_MANIFEST_DIR"));
    cmd.env("TERM", "xterm-256color");
    cmd.env("SHELL", "/bin/sh");
    // `DIRK_SHOT_CONFIG=<dir> cargo run --example shot` points dirk at a
    // configuration directory of your own, which is how a layout gets looked at
    // without installing it.
    if let Ok(dir) = std::env::var("DIRK_SHOT_CONFIG") {
        cmd.env("XDG_CONFIG_HOME", dir);
    }
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

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

    // Let it come up, then give it something to look at: two workspaces with
    // titles for naming to work from, and a split to show the layout tree.
    //
    // The split comes last on purpose. `name::decide` skips a workspace holding
    // more than one pane -- two panes have no single intent -- so splitting
    // before the title has settled would leave that workspace showing its
    // project name forever, and the shot would quietly stop demonstrating the
    // one feature it was written for.
    // `clear` on the same line: the printf is scaffolding for the shot, and a
    // pane showing the command that titled it is showing the scaffolding.
    let title = |t: &str| format!("printf '\\033]2;{t}\\007'; clear\r");
    let prefix = |k: &[u8]| [&[0u8][..], k].concat();

    std::thread::sleep(Duration::from_millis(700));
    writer
        .write_all(title("Building the mux core").as_bytes())
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));

    writer.write_all(&prefix(b"n")).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    writer
        .write_all(title("Reading the vt100 grid").as_bytes())
        .unwrap();

    // Longer than the naming debounce, so both names are committed.
    std::thread::sleep(Duration::from_millis(2000));

    writer.write_all(&prefix(b"|")).unwrap();
    // Longer than a tick, so what is running in the new pane has been sampled
    // and its state glyph is settled rather than still unknown.
    std::thread::sleep(Duration::from_millis(1800));

    if std::env::var("DIRK_SHOT_LAYOUT").is_ok() {
        writer.write_all(&prefix(b"1")).unwrap();
        std::thread::sleep(Duration::from_millis(2500));
    }

    if std::env::args().any(|a| a == "--html") {
        print!("{}", html(&screen.lock().unwrap(), rows, cols));
    } else {
        for row in 0..rows {
            let s = screen.lock().unwrap();
            let line: String = (0..cols)
                .map(|c| {
                    s.screen()
                        .cell(row, c)
                        .map(|x| {
                            let t = x.contents();
                            if t.is_empty() {
                                " ".to_string()
                            } else {
                                t.to_string()
                            }
                        })
                        .unwrap_or_else(|| " ".into())
                })
                .collect();
            println!("│{}│", line.trim_end());
        }
    }

    writer.write_all(&[0]).unwrap();
    writer.write_all(b"q").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = child.kill();
}

// ─── The same screen, with its colours ──────────────────────────────────────

/// The screen as HTML: one span per run of cells sharing a style.
///
/// Per-cell spans would be correct and four times the size. Runs are what make
/// the difference between a render that is a few kilobytes and one that is not
/// worth shipping, and a terminal line is mostly runs.
fn html(parser: &vt100::Parser, rows: u16, cols: u16) -> String {
    let screen = parser.screen();
    let mut out = String::with_capacity(64 * 1024);

    out.push_str(
        "<figure class=\"term\" aria-label=\"dirk, running: the nav on the left, panes on the right\">\n",
    );
    out.push_str("<div class=\"term-bar\"><span class=\"term-dot\"></span>");
    out.push_str("<span class=\"term-name\">dirk</span></div>\n");
    out.push_str("<pre class=\"term-screen\">");

    for row in 0..rows {
        // A run is open while the style holds. `open` is the style it was
        // opened with, so a change closes it and a match extends it.
        let mut open: Option<String> = None;
        let mut line = String::new();

        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            let style = style_of(cell);
            if open.as_deref() != Some(style.as_str()) {
                // Only a run that opened a span has one to close. A run of
                // default-coloured cells opens nothing, and closing it anyway
                // is a stray tag that browsers forgive and validators do not.
                if open.as_deref().is_some_and(|s| !s.is_empty()) {
                    line.push_str("</span>");
                }
                if !style.is_empty() {
                    line.push_str(&format!("<span style=\"{style}\">"));
                }
                open = Some(style);
            }
            // A cell with no contents is a blank the grid is still holding
            // open; it has to be written, or the line shortens under it.
            let text = cell.contents();
            if text.is_empty() {
                line.push(' ');
            } else {
                escape_into(text, &mut line);
            }
        }
        if open.as_deref().is_some_and(|s| !s.is_empty()) {
            line.push_str("</span>");
        }

        // Trailing blanks are not information, and on a 92-column screen there
        // are a great many of them.
        out.push_str(line.trim_end());
        out.push('\n');
    }

    out.push_str("</pre>\n</figure>\n");
    out
}

fn style_of(cell: &vt100::Cell) -> String {
    let mut style = String::new();
    if let Some(fg) = colour(cell.fgcolor()) {
        style.push_str(&format!("color:{fg};"));
    }
    if let Some(bg) = colour(cell.bgcolor()) {
        style.push_str(&format!("background:{bg};"));
    }
    if cell.bold() {
        style.push_str("font-weight:600;");
    }
    if cell.italic() {
        style.push_str("font-style:italic;");
    }
    style
}

/// A terminal colour as CSS, or nothing at all.
///
/// `Default` returns `None` rather than a colour: the frame already paints the
/// ground and the text, and writing them again on every cell would be a third
/// of the file for no change on the screen.
fn colour(c: vt100::Color) -> Option<String> {
    match c {
        vt100::Color::Default => None,
        vt100::Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        vt100::Color::Idx(i) => Some(indexed(i)),
    }
}

/// The xterm 256-colour cube. dirk names its own colours in RGB, so anything
/// indexed came from a program running inside a pane.
fn indexed(i: u8) -> String {
    const ANSI: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00),
        (0xcd, 0x00, 0x00),
        (0x00, 0xcd, 0x00),
        (0xcd, 0xcd, 0x00),
        (0x00, 0x00, 0xee),
        (0xcd, 0x00, 0xcd),
        (0x00, 0xcd, 0xcd),
        (0xe5, 0xe5, 0xe5),
        (0x7f, 0x7f, 0x7f),
        (0xff, 0x00, 0x00),
        (0x00, 0xff, 0x00),
        (0xff, 0xff, 0x00),
        (0x5c, 0x5c, 0xff),
        (0xff, 0x00, 0xff),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0xff),
    ];
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

    let (r, g, b) = match i {
        0..=15 => ANSI[i as usize],
        16..=231 => {
            let n = i - 16;
            (
                LEVELS[(n / 36) as usize],
                LEVELS[((n % 36) / 6) as usize],
                LEVELS[(n % 6) as usize],
            )
        }
        _ => {
            let v = 8 + 10 * (i - 232);
            (v, v, v)
        }
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn escape_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}
