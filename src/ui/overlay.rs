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

//! A panel over the panes: a prompt, a rule, and a list under it.
//!
//! The picker, the palette and the find results are the same panel three times
//! — the same box arithmetic, the same prompt row, the same rule beneath it,
//! the same first-row-is-two-down, the same strip filled when it is the chosen
//! one, the same hit registered on it. What differs is what goes *in* a row,
//! which is the part each of them keeps.
//!
//! It matters more than the line count. The arithmetic was written three times
//! and tested none, so a change to how an overlay looks had to be made three
//! times with nothing to catch the copy that was missed. Here it is one thing
//! with tests under it.

use crate::glyph::{G, Glyphs};
use crate::hit::{HitMap, Target};
use crate::theme::THEME;
use crate::ui::{cells, fill, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// A box in the middle of the screen, small enough to see past.
///
/// `down` divides the room above and below: 3 puts the box a third of the way
/// down, which reads as centred without being centred — the eye takes the
/// middle of a screen to be above the middle of a screen.
pub fn centred(area: Rect, width: (u16, u16), height: (u16, u16), down: u16) -> Rect {
    let w = area.width.clamp(width.0, width.1).min(area.width);
    let h = area.height.clamp(height.0, height.1).min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / down.max(1),
        width: w,
        height: h,
    }
}

/// A box along the bottom, the full width of what it is over.
///
/// For a list you are reading *while* looking at what it refers to: anchored
/// where the eye already is after typing, and wide because the rows are lines
/// of somebody's output rather than names.
pub fn along_the_bottom(area: Rect, height: (u16, u16)) -> Rect {
    let h = area.height.clamp(height.0, height.1).min(area.height);
    Rect {
        x: area.x,
        y: area.bottom().saturating_sub(h),
        width: area.width,
        height: h,
    }
}

/// An opened panel: where its rows go and which of them are on screen.
pub struct Overlay {
    /// The whole box.
    pub at: Rect,
    /// Where content starts, inside the margin.
    pub x: u16,
    /// How much room content has.
    pub inner: u16,
    /// How many rows fit under the prompt and its rule.
    pub rows: usize,
    /// The first item drawn, chosen to keep the selected one in view.
    pub first: usize,
}

impl Overlay {
    /// Draw the panel and its heading, and work out which items fit.
    ///
    /// The window is the part worth having in one place: `first` is chosen so
    /// the selection is always on screen, which two of these three did and the
    /// third did not — so holding an arrow down in the picker walked the
    /// selection off the bottom of a list that never scrolled.
    pub fn open(
        buf: &mut Buffer,
        g: &Glyphs,
        at: Rect,
        prompt: &str,
        selected: usize,
        count: usize,
    ) -> Self {
        fill(buf, at, THEME.panel());

        let inner = at.width.saturating_sub(4);
        let x = at.x + 2;

        // A block cursor, since the real one is parked on a pane behind this.
        let used = write_str(buf, x, at.y, prompt, THEME.text(), inner);
        write_str(
            buf,
            x + used,
            at.y,
            g.text(G::BarFocused),
            THEME.working(),
            inner.saturating_sub(used),
        );

        for col in 0..inner {
            write_str(
                buf,
                x + col,
                at.y + 1,
                g.text(G::Rule),
                THEME.rule_strong(),
                inner,
            );
        }

        let rows = usize::from(at.height.saturating_sub(2));
        let first = selected.saturating_sub(rows.saturating_sub(1)).min(count);
        Overlay {
            at,
            x,
            inner,
            rows,
            first,
        }
    }

    /// Right-aligned on the prompt row: a count, or anything else short enough
    /// to belong beside what was typed rather than under it.
    pub fn tally(&self, buf: &mut Buffer, text: &str) {
        if text.is_empty() {
            return;
        }
        let room = cells(text);
        write_str(
            buf,
            self.at.right().saturating_sub(room + 2),
            self.at.y,
            text,
            THEME.faint(),
            room,
        );
    }

    /// Which items are on screen, as indices into the whole list.
    pub fn showing(&self, count: usize) -> std::ops::Range<usize> {
        self.first..count.min(self.first + self.rows)
    }

    /// The strip for one visible row: filled when it is the chosen one, and
    /// clickable either way.
    ///
    /// `row` counts from the top of the list as drawn, not from the top of the
    /// items — the difference is `first`, and getting it wrong puts the
    /// highlight on the wrong line once a list has scrolled.
    pub fn strip(
        &self,
        buf: &mut Buffer,
        hits: &mut HitMap,
        row: usize,
        chosen: bool,
        target: Target,
    ) -> Strip {
        let y = self.at.y + 2 + row as u16;
        let full = Rect {
            x: self.at.x,
            y,
            width: self.at.width,
            height: 1,
        };
        if chosen {
            fill(buf, full, THEME.selected());
        }
        hits.push(full, target);
        Strip {
            y,
            x: self.x,
            room: self.inner,
            chosen,
        }
    }
}

/// Where one row's content goes.
pub struct Strip {
    pub y: u16,
    /// Where to start writing. Moves right as a row lays out its own columns.
    pub x: u16,
    /// What is left of the row, in cells.
    pub room: u16,
    pub chosen: bool,
}

impl Strip {
    /// Write, and move along by what was written.
    pub fn write(&mut self, buf: &mut Buffer, text: &str, style: ratatui::style::Style) {
        let used = write_str(buf, self.x, self.y, text, style, self.room);
        self.x += used;
        self.room = self.room.saturating_sub(used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs() -> Glyphs {
        Glyphs::set("unicode")
    }

    fn screen() -> (Buffer, Rect) {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        (Buffer::empty(area), area)
    }

    #[test]
    fn a_centred_box_is_inside_what_it_is_over() {
        let (_, area) = screen();
        for down in [2, 3, 4] {
            let at = centred(area, (20, 56), (3, 18), down);
            assert!(at.x >= area.x && at.right() <= area.right());
            assert!(at.y >= area.y && at.bottom() <= area.bottom());
        }
    }

    #[test]
    fn a_box_fits_a_screen_too_small_for_its_minimum() {
        // A terminal narrower than the smallest box asked for. Clamping the
        // other way round would put the box off the side of the screen, and
        // the arithmetic that follows would underflow rather than clip.
        let tiny = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 2,
        };
        let at = centred(tiny, (20, 56), (3, 18), 3);
        assert!(at.width <= tiny.width && at.height <= tiny.height);
        assert!(at.right() <= tiny.right() && at.bottom() <= tiny.bottom());

        let bottom = along_the_bottom(tiny, (3, 12));
        assert!(bottom.height <= tiny.height);
        assert_eq!(bottom.bottom(), tiny.bottom());
    }

    #[test]
    fn the_selection_is_always_on_screen() {
        // The bug this had in the picker: a list that never scrolled, so
        // holding an arrow down walked the selection off the bottom of it.
        let (mut buf, area) = screen();
        let at = centred(area, (20, 56), (3, 18), 3);
        let count = 200;
        for selected in [0, 1, 17, 18, 100, 199] {
            let o = Overlay::open(&mut buf, &glyphs(), at, "open  ", selected, count);
            let showing = o.showing(count);
            assert!(
                showing.contains(&selected),
                "selection {selected} is not among {showing:?}"
            );
        }
    }

    #[test]
    fn a_short_list_starts_at_the_top() {
        let (mut buf, area) = screen();
        let at = centred(area, (20, 56), (3, 18), 3);
        let o = Overlay::open(&mut buf, &glyphs(), at, "open  ", 0, 3);
        assert_eq!(o.first, 0);
        assert_eq!(o.showing(3), 0..3);
    }

    #[test]
    fn an_empty_list_shows_nothing_rather_than_underflowing() {
        let (mut buf, area) = screen();
        let at = centred(area, (20, 56), (3, 18), 3);
        let o = Overlay::open(&mut buf, &glyphs(), at, "open  ", 0, 0);
        assert!(o.showing(0).is_empty());
    }

    #[test]
    fn a_row_is_clickable_where_it_is_drawn() {
        let (mut buf, area) = screen();
        let mut hits = HitMap::default();
        let at = centred(area, (20, 56), (3, 18), 3);
        let o = Overlay::open(&mut buf, &glyphs(), at, "open  ", 0, 5);
        let strip = o.strip(&mut buf, &mut hits, 2, false, Target::PickerRow(2));
        // Two down from the prompt, plus the two the prompt and its rule take.
        assert_eq!(strip.y, at.y + 4);
        assert!(matches!(hits.at(at.x, strip.y), Some(Target::PickerRow(2))));
    }
}
