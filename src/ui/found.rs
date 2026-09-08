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
use crate::ui::elide;
use crate::ui::overlay::{self, Overlay};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    g: &crate::glyph::Glyphs,
    find: &Find,
    hits: &mut HitMap,
) {
    let all = find.hits();
    let at = overlay::along_the_bottom(area, (3, 12));
    let panel = Overlay::open(
        buf,
        g,
        at,
        &format!("find  {}", find.query),
        find.at,
        all.len(),
    );

    // The count belongs beside what was typed: an incremental search with no
    // idea how many it found is one you keep typing at hopefully.
    panel.tally(
        buf,
        &match all.len() {
            0 if find.query.is_empty() => String::new(),
            0 => "none".into(),
            n => format!("{} of {n}", find.at + 1),
        },
    );

    for row in panel.showing(all.len()) {
        let Some(line) = find.line(all[row]) else {
            continue;
        };
        let mut strip = panel.strip(
            buf,
            hits,
            row - panel.first,
            row == find.at,
            Target::FoundRow(row),
        );

        // The pane first and fixed-width, so the results line up as a column
        // you can read down rather than as ragged prose.
        const WHERE: usize = 22;
        let from = elide(&line.where_from, WHERE, g.text(G::Ellipsis));
        let style = match strip.chosen {
            true => THEME.selected(),
            false => THEME.project(),
        };
        strip.write(buf, &format!("{from:<WHERE$}"), style);
        strip.write(buf, "  ", THEME.faint());

        let style = match strip.chosen {
            true => THEME.selected(),
            false => THEME.dim(),
        };
        let room = strip.room.saturating_sub(2) as usize;
        strip.write(
            buf,
            &elide(line.text.trim(), room, g.text(G::Ellipsis)),
            style,
        );
    }
}
