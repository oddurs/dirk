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

//! The chrome: everything dirk draws that is not a pane.

pub mod pane;
pub mod picker;
pub mod rail;
pub mod sidebar;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

/// A section heading: upper-cased and letter-spaced, so it reads as structure
/// rather than as the first row of the list under it.
pub fn heading(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
    let spaced: String = text
        .to_uppercase()
        .chars()
        .flat_map(|c| [c, ' '])
        .collect::<String>()
        .trim_end()
        .into();
    write_str(buf, area.x, area.y, &spaced, style, area.width);
}

/// Write a string into the buffer, clipped to `max` columns.
pub fn write_str(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style, max: u16) -> u16 {
    let mut col = 0u16;
    for ch in s.chars() {
        if col >= max {
            break;
        }
        let Some(cell) = buf.cell_mut((x + col, y)) else {
            break;
        };
        cell.set_char(ch);
        cell.set_style(style);
        col += 1;
    }
    col
}

/// Shorten with an ellipsis, so a long intent truncates where it is read
/// rather than where the column happens to end.
pub fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    s.chars().take(max - 1).collect::<String>() + "…"
}

/// Paint a whole rect with one style, so a surface reads as a surface.
pub fn fill(buf: &mut Buffer, area: Rect, style: Style) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(' ');
                c.set_style(style);
            }
        }
    }
}
