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

//! One line of text, asked for.
//!
//! The project picker is the right shape for choosing from a list that already
//! exists. This is the other case: the answer is a thing you are about to make,
//! so there is nothing to list and nothing to filter.

use crate::glyph::G;
use crate::theme::THEME;
use crate::ui::{cells, fill, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn render(buf: &mut Buffer, area: Rect, g: &crate::glyph::Glyphs, what: &str, text: &str) {
    let w = area.width.clamp(20, 56).min(area.width);
    let box_area = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + area.height / 3,
        width: w,
        height: 3.min(area.height),
    };
    fill(buf, box_area, THEME.panel());

    let inner = w.saturating_sub(4);
    let x = box_area.x + 2;
    write_str(buf, x, box_area.y, what, THEME.title(), inner);

    let mut cx = x;
    cx += write_str(buf, cx, box_area.y + 1, text, THEME.text(), inner);
    // A block cursor, since the real one is parked on a pane behind this.
    write_str(
        buf,
        cx,
        box_area.y + 1,
        g.text(G::BarFocused),
        THEME.working(),
        inner.saturating_sub(cells(text)),
    );

    write_str(
        buf,
        x,
        box_area.y + 2,
        "enter makes it  ·  esc leaves",
        THEME.faint(),
        inner,
    );
}
