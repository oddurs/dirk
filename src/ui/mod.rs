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

pub mod nav;
pub mod pane;
pub mod picker;
pub mod rail;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::UnicodeWidthChar;

/// A section heading.
///
/// Plain text in the accent colour. These were letter-spaced upper case once,
/// which reads as decoration rather than as a label and makes every heading
/// twice as wide as the word in it — expensive in a column this narrow.
pub fn heading(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
    write_str(buf, area.x, area.y, text, style, area.width);
}

/// How many columns a string occupies.
///
/// Not how many characters it has. A workspace named in Japanese, or one
/// carrying an emoji an agent put in its own title, takes two columns per
/// character -- and every width in the nav is reserved before the name that
/// has to fit in it.
pub fn cells(s: &str) -> u16 {
    s.chars().map(cell_width).sum()
}

fn cell_width(ch: char) -> u16 {
    UnicodeWidthChar::width(ch).unwrap_or(0) as u16
}

/// Write a string into the buffer, clipped to `max` columns.
///
/// Answers the columns used, which is what every caller adds to its cursor.
pub fn write_str(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style, max: u16) -> u16 {
    let mut col = 0u16;
    for ch in s.chars() {
        let width = cell_width(ch);
        // Combining marks and zero-width joiners: they belong to the character
        // before them, and dropping them is better than giving them a column.
        if width == 0 {
            continue;
        }
        if col + width > max {
            break;
        }
        let Some(cell) = buf.cell_mut((x + col, y)) else {
            break;
        };
        cell.set_char(ch);
        cell.set_style(style);
        // A wide character owns the cell after it as well. Left alone, that
        // cell keeps whatever the last frame put there and shows through.
        if width == 2
            && let Some(tail) = buf.cell_mut((x + col + 1, y))
        {
            tail.set_symbol("");
            tail.set_style(style);
        }
        col += width;
    }
    col
}

/// Shorten with an ellipsis, so a long intent truncates where it is read
/// rather than where the column happens to end.
///
/// The ellipsis comes from the caller because it is a glyph like any other,
/// and a set chosen for a terminal that cannot draw `▾` cannot draw `…` either.
pub fn elide(s: &str, max: usize, ellipsis: &str) -> String {
    if cells(s) as usize <= max {
        return s.to_string();
    }
    let mark = cells(ellipsis) as usize;
    if max <= mark {
        return ellipsis.to_string();
    }
    let budget = max - mark;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let width = cell_width(ch) as usize;
        if used + width > budget {
            break;
        }
        out.push(ch);
        used += width;
    }
    out.push_str(ellipsis);
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    fn strip(buf: &Buffer, width: u16) -> String {
        (0..width)
            .map(|x| buf.cell((x, 0)).map_or(" ", |c| c.symbol()).to_string())
            .collect()
    }

    #[test]
    fn a_wide_character_is_two_columns_and_not_one() {
        // Every width in the nav is reserved before the name that has to fit in
        // it, so counting characters where the terminal counts columns puts the
        // age on top of the name -- and an agent that puts an emoji in its own
        // title is all it takes.
        assert_eq!(cells("abc"), 3);
        assert_eq!(cells("読み込み"), 8, "four characters, eight columns");
        assert_eq!(cells("🙂"), 2);
        // Combining marks belong to the character before them.
        assert_eq!(cells("e\u{301}"), 1);
    }

    #[test]
    fn a_clip_never_writes_half_of_a_wide_character() {
        // Half of one is not half as wide, it is a cell the terminal fills with
        // something of its own choosing and the column after it moves.
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 1));
        let used = write_str(&mut buf, 0, 0, "読み", Style::default(), 3);
        assert_eq!(
            used, 2,
            "the second character did not fit and was not drawn"
        );
        // Cell 0 holds it, cell 1 is its claimed tail, and nothing was written
        // into the third column it did not have room for.
        assert_eq!(buf.cell((2, 0)).map(|c| c.symbol()), Some(" "));
        assert!(
            !strip(&buf, 6).contains('み'),
            "a half character was written"
        );
    }

    #[test]
    fn the_cell_after_a_wide_character_is_claimed() {
        // Left alone it keeps whatever the last frame put there, and shows
        // through beside the character that is supposed to own it.
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        write_str(&mut buf, 0, 0, "xxxx", Style::default(), 4);
        write_str(&mut buf, 0, 0, "読", Style::default(), 4);
        assert_eq!(
            buf.cell((1, 0)).map(|c| c.symbol()),
            Some(""),
            "the tail of a wide character showed the frame before it"
        );
    }

    #[test]
    fn eliding_measures_columns_and_keeps_the_mark() {
        assert_eq!(elide("abcdef", 4, "…"), "abc…");
        // Three wide characters is six columns; four columns leaves room for
        // one of them and the ellipsis.
        assert_eq!(elide("読み込", 4, "…"), "読…");
        // A set chosen for a terminal that cannot draw `▾` cannot draw `…`.
        assert_eq!(elide("abcdef", 4, "~"), "abc~");
        // Nothing to elide is left exactly alone.
        assert_eq!(elide("abc", 3, "…"), "abc");
    }
}
