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

//! Painting a pane, and sending the mouse back into it.
//!
//! `vt100` keeps a grid of styled cells; ratatui wants a buffer of styled
//! cells. This module is the copy between them and nothing else — no borders,
//! no title, no state. Panes paint no background of their own: whatever the
//! program inside drew is what shows, which is what keeps chrome and content
//! visibly different surfaces.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Copy one terminal grid into `area`.
///
/// `dim` washes out an unfocused pane. It is applied as a style modifier
/// rather than by mixing colours, so a pane that comes back into focus is
/// pixel-identical to the one that left it.
/// The same, with whatever is selected marked.
///
/// Reversed rather than recoloured: a selection has to be visible over whatever
/// the program chose, and there is no colour that is legible on all of them.
pub fn paint(
    screen: &vt100::Screen,
    area: Rect,
    buf: &mut Buffer,
    dim: bool,
    span: Option<crate::copy::Span>,
) {
    let (rows, cols) = screen.size();
    let h = area.height.min(rows);
    let w = area.width.min(cols);

    for r in 0..h {
        for c in 0..w {
            let Some(src) = screen.cell(r, c) else {
                continue;
            };
            // The second half of a double-width character owns no buffer cell;
            // the wide symbol in the previous one covers both columns.
            if src.is_wide_continuation() {
                continue;
            }
            let Some(dst) = buf.cell_mut((area.x + c, area.y + r)) else {
                continue;
            };

            let s = src.contents();
            dst.set_symbol(if s.is_empty() { " " } else { s });

            let mut st = Style::default()
                .fg(color(src.fgcolor()))
                .bg(color(src.bgcolor()));
            if src.bold() {
                st = st.add_modifier(Modifier::BOLD);
            }
            if src.italic() {
                st = st.add_modifier(Modifier::ITALIC);
            }
            if src.underline() {
                st = st.add_modifier(Modifier::UNDERLINED);
            }
            if src.inverse() {
                st = st.add_modifier(Modifier::REVERSED);
            }
            if dim {
                st = st.add_modifier(Modifier::DIM);
            }
            // Reversed on top of whatever the program chose, and reversed
            // *again* if it had already inverted the cell -- so a selection
            // over a highlighted line is still visibly a selection.
            if span.is_some_and(|s| s.holds(r, c)) {
                st = match src.inverse() {
                    true => st.remove_modifier(Modifier::REVERSED),
                    false => st.add_modifier(Modifier::REVERSED),
                };
            }
            dst.set_style(st);

            // Blank the cell the wide character spills into, or ratatui will
            // draw whatever was there before alongside it.
            if src.is_wide()
                && let Some(next) = buf.cell_mut((area.x + c + 1, area.y + r))
            {
                next.set_symbol("");
            }
        }
    }
}

/// Where the cursor should sit, in screen coordinates, or `None` when the
/// program has hidden it.
pub fn cursor(screen: &vt100::Screen, area: Rect) -> Option<(u16, u16)> {
    if screen.hide_cursor() {
        return None;
    }
    let (r, c) = screen.cursor_position();
    if r >= area.height || c >= area.width {
        return None;
    }
    Some((area.x + c, area.y + r))
}

/// Encode a mouse event the way the program inside asked to receive it.
///
/// Answering this per pane is what lets one click mean "focus this pane" in a
/// shell and "click this line" in lazygit, with no mode for the human to
/// remember: if the program turned mouse reporting on, it gets the event.
///
/// `col`/`row` are already pane-relative and zero-based.
pub fn encode_mouse(
    screen: &vt100::Screen,
    ev: &MouseEvent,
    col: u16,
    row: u16,
) -> Option<Vec<u8>> {
    use vt100::{MouseProtocolEncoding as Enc, MouseProtocolMode as Mode};

    let mode = screen.mouse_protocol_mode();
    if mode == Mode::None {
        return None;
    }

    let (mut cb, press) = match ev.kind {
        MouseEventKind::Down(b) => (button(b), true),
        MouseEventKind::Up(b) => (button(b), false),
        // Motion is only wanted by the modes that asked for it.
        MouseEventKind::Drag(b) => {
            if mode != Mode::ButtonMotion && mode != Mode::AnyMotion {
                return None;
            }
            (button(b) + 32, true)
        }
        MouseEventKind::Moved => {
            if mode != Mode::AnyMotion {
                return None;
            }
            (35, true)
        }
        MouseEventKind::ScrollUp => (64, true),
        MouseEventKind::ScrollDown => (65, true),
        MouseEventKind::ScrollLeft => (66, true),
        MouseEventKind::ScrollRight => (67, true),
    };

    // X10 mode reports presses only.
    if mode == Mode::Press && !press {
        return None;
    }

    let m = ev.modifiers;
    if m.contains(crossterm::event::KeyModifiers::SHIFT) {
        cb += 4;
    }
    if m.contains(crossterm::event::KeyModifiers::ALT) {
        cb += 8;
    }
    if m.contains(crossterm::event::KeyModifiers::CONTROL) {
        cb += 16;
    }

    // Terminal coordinates are 1-based.
    let (c1, r1) = (col + 1, row + 1);

    Some(match screen.mouse_protocol_encoding() {
        Enc::Sgr => format!("\x1b[<{cb};{c1};{r1}{}", if press { 'M' } else { 'm' }).into_bytes(),
        // The original encoding cannot express a release button or a
        // coordinate past 223, which is exactly why SGR exists.
        _ => {
            if c1 > 223 || r1 > 223 {
                return None;
            }
            let cb = if press { cb } else { 3 };
            vec![
                0x1b,
                b'[',
                b'M',
                32 + cb as u8,
                32 + c1 as u8,
                32 + r1 as u8,
            ]
        }
    })
}

fn button(b: MouseButton) -> u16 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}
