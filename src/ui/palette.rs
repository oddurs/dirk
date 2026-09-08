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
use crate::ui::overlay::{self, Overlay};
use crate::ui::{cells, elide, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    g: &crate::glyph::Glyphs,
    palette: &Palette,
    hits: &mut HitMap,
) {
    let all = palette.matches();
    let at = overlay::centred(area, (30, 64), (3, 16), 4);
    let panel = Overlay::open(
        buf,
        g,
        at,
        &format!("do  {}", palette.query),
        palette.selected,
        all.len(),
    );

    for row in panel.showing(all.len()) {
        let (index, entry) = &all[row];
        let mut strip = panel.strip(
            buf,
            hits,
            row - panel.first,
            row == palette.selected,
            Target::PaletteRow(*index),
        );

        // The key first, right-aligned in its own narrow column, so the eye can
        // run down it looking for the one thing worth remembering.
        const KEY: usize = 3;
        strip.write(buf, &format!("{:>KEY$}", entry.key), THEME.key());
        strip.write(buf, "  ", THEME.faint());

        // The reason is reserved before the label, so a long label elides
        // rather than running over the thing that says why it is greyed out.
        let mut reserved = 0u16;
        if let Some(why) = &entry.why_not {
            let why = elide(why, 20, g.text(G::Ellipsis));
            reserved = cells(&why) + 2;
            write_str(
                buf,
                panel.at.right().saturating_sub(reserved),
                strip.y,
                &why,
                THEME.faint(),
                cells(&why),
            );
        }

        let style = match (strip.chosen, entry.why_not.is_some()) {
            (true, _) => THEME.selected(),
            // Shown rather than hidden: a palette that omits what it cannot do
            // teaches you that dirk cannot do it.
            (false, true) => THEME.faint(),
            (false, false) => THEME.text(),
        };
        let room = strip.room.saturating_sub(reserved + 2) as usize;
        strip.write(buf, &elide(&entry.label, room, g.text(G::Ellipsis)), style);
    }
}
