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

//! What a search found, over whatever you were doing.
//!
//! Wider than the project picker and pinned to the bottom, because these are
//! lines of output rather than names: they want the width, and the thing you
//! are searching is usually above them.
//!
//! Each result says which pane it came from. The line you are looking for is
//! usually in the pane you were not watching, which is the entire reason to
//! search across all of them.

use crate::find::Find;
use crate::glyph::G;
use crate::hit::{HitMap, Target};
use crate::theme::THEME;
use crate::ui::{cells, elide, fill, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    g: &crate::glyph::Glyphs,
    find: &Find,
    hits: &mut HitMap,
) {
    let h = area.height.clamp(3, 12).min(area.height);
    let box_area = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(h),
        width: area.width,
        height: h,
    };
    fill(buf, box_area, THEME.panel());

    let inner_w = box_area.width.saturating_sub(4);
    let x = box_area.x + 2;

    // The count is part of the prompt: an incremental search with no idea how
    // many it found is one you keep typing at hopefully.
    let found = find.hits().len();
    let prompt = format!("find  {}", find.query);
    let mut px = write_str(buf, x, box_area.y, &prompt, THEME.text(), inner_w);
    px += write_str(
        buf,
        x + px,
        box_area.y,
        "▌",
        THEME.working(),
        inner_w.saturating_sub(px),
    );
    let tally = match found {
        0 if find.query.is_empty() => String::new(),
        0 => "none".into(),
        n => format!("{} of {n}", find.at + 1),
    };
    write_str(
        buf,
        box_area
            .right()
            .saturating_sub(cells(&tally).saturating_add(2)),
        box_area.y,
        &tally,
        THEME.faint(),
        inner_w.saturating_sub(px),
    );

    for col in 0..inner_w {
        write_str(
            buf,
            x + col,
            box_area.y + 1,
            g.text(G::Rule),
            THEME.rule_strong(),
            inner_w,
        );
    }

    // A window that keeps the current match in it, so stepping past the bottom
    // does not walk off the list.
    let rows = h.saturating_sub(2) as usize;
    let first = find.at.saturating_sub(rows.saturating_sub(1));
    for (row, (index, hit)) in find
        .hits()
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
        .enumerate()
    {
        let Some(line) = find.line(*hit) else {
            continue;
        };
        let y = box_area.y + 2 + row as u16;
        let full = Rect {
            x: box_area.x,
            y,
            width: box_area.width,
            height: 1,
        };
        let here = index == find.at;
        if here {
            fill(buf, full, THEME.selected());
        }

        // The pane first and fixed-width, so the results line up as a column
        // you can read down rather than as ragged prose.
        const WHERE: usize = 22;
        let from = elide(&line.where_from, WHERE, g.text(G::Ellipsis));
        let mut cx = x;
        cx += write_str(
            buf,
            cx,
            y,
            &format!("{from:<WHERE$}"),
            match here {
                true => THEME.selected(),
                false => THEME.project(),
            },
            inner_w,
        );
        cx += write_str(buf, cx, y, "  ", THEME.faint(), inner_w);

        let left = box_area.right().saturating_sub(cx + 2) as usize;
        write_str(
            buf,
            cx,
            y,
            &elide(line.text.trim(), left, g.text(G::Ellipsis)),
            match here {
                true => THEME.selected(),
                false => THEME.dim(),
            },
            inner_w,
        );
        hits.push(full, Target::FoundRow(index));
    }
}
