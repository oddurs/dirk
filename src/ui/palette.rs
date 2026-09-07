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

//! Everything dirk can do, over whatever you were doing.
//!
//! Three columns: what it is, why it cannot be done, and what to press instead
//! of coming here. The last is the point — a palette that does not teach you
//! the key is one you keep coming back to.

use crate::glyph::G;
use crate::hit::{HitMap, Target};
use crate::palette::Palette;
use crate::theme::THEME;
use crate::ui::{cells, elide, fill, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    g: &crate::glyph::Glyphs,
    palette: &Palette,
    hits: &mut HitMap,
) {
    let w = area.width.clamp(30, 64).min(area.width);
    let h = area.height.clamp(3, 16).min(area.height);
    let box_area = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 4,
        width: w,
        height: h,
    };
    fill(buf, box_area, THEME.panel());

    let inner = w.saturating_sub(4);
    let x = box_area.x + 2;

    let prompt = format!("do  {}", palette.query);
    let px = write_str(buf, x, box_area.y, &prompt, THEME.text(), inner);
    write_str(
        buf,
        x + px,
        box_area.y,
        g.text(G::BarFocused),
        THEME.working(),
        inner.saturating_sub(px),
    );

    for col in 0..inner {
        write_str(
            buf,
            x + col,
            box_area.y + 1,
            g.text(G::Rule),
            THEME.rule_strong(),
            inner,
        );
    }

    let rows = h.saturating_sub(2) as usize;
    let all = palette.matches();
    // A window that keeps the selection in it, so holding down the arrow does
    // not walk off the end of what is drawn.
    let first = palette.selected.saturating_sub(rows.saturating_sub(1));
    for (row, (index, entry)) in all.iter().skip(first).take(rows).enumerate() {
        let at = first + row;
        let y = box_area.y + 2 + row as u16;
        let full = Rect {
            x: box_area.x,
            y,
            width: w,
            height: 1,
        };
        let here = at == palette.selected;
        if here {
            fill(buf, full, THEME.selected());
        }

        // The key first, right-aligned in its own narrow column, so the eye can
        // run down it looking for the one thing worth remembering.
        const KEY: usize = 3;
        let key = format!("{:>KEY$}", entry.key);
        let mut cx = x;
        cx += write_str(buf, cx, y, &key, THEME.key(), inner);
        cx += write_str(buf, cx, y, "  ", THEME.faint(), inner);

        // The reason is reserved before the label, so a long label elides
        // rather than running over the thing that says why it is greyed out.
        let mut right = 0u16;
        if let Some(why) = &entry.why_not {
            let why = elide(why, 20, g.text(G::Ellipsis));
            right = cells(&why) + 2;
            write_str(
                buf,
                box_area.right().saturating_sub(cells(&why) + 2),
                y,
                &why,
                THEME.faint(),
                cells(&why),
            );
        }

        let left = box_area
            .right()
            .saturating_sub(cx + 2)
            .saturating_sub(right) as usize;
        let style = match (here, entry.why_not.is_some()) {
            (true, _) => THEME.selected(),
            // Shown rather than hidden: a palette that omits what it cannot do
            // teaches you that dirk cannot do it.
            (false, true) => THEME.faint(),
            (false, false) => THEME.text(),
        };
        write_str(
            buf,
            cx,
            y,
            &elide(&entry.label, left, g.text(G::Ellipsis)),
            style,
            inner,
        );
        hits.push(full, Target::PaletteRow(*index));
    }
}
