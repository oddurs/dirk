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

//! Reading what has gone past, and taking some of it with you.
//!
//! Two things that look like one. Scrolling is a property of a pane — it is
//! where that pane is being read from, and it survives you looking elsewhere.
//! Selecting is a mode: while it is on, keys move a cursor instead of reaching
//! the program, and that has to end.
//!
//! Neither touches the program inside. Scrolling moves vt100's window over a
//! grid it is already keeping, and selecting reads cells. Nothing is written to
//! the pty, so a build does not learn that you looked at it.

use crate::mux::PaneId;

/// Where a selection is, in the coordinates of what is currently drawn.
///
/// Screen coordinates rather than scrollback ones, because the selection is a
/// thing you are pointing at. Scrolling while selecting moves the text under
/// the selection, which is what the same gesture does in every terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: (u16, u16),
    pub to: (u16, u16),
}

impl Span {
    /// The two ends in reading order, so a selection dragged upwards reads the
    /// same as one dragged down.
    pub fn ordered(self) -> ((u16, u16), (u16, u16)) {
        let (a, b) = (self.from, self.to);
        // Row first: a selection is lines of text, not a rectangle.
        match (a.0, a.1) <= (b.0, b.1) {
            true => (a, b),
            false => (b, a),
        }
    }

    /// Whether a cell is inside, so the renderer can mark it.
    pub fn holds(self, row: u16, col: u16) -> bool {
        let ((r0, c0), (r1, c1)) = self.ordered();
        if row < r0 || row > r1 {
            return false;
        }
        if r0 == r1 {
            return col >= c0 && col <= c1;
        }
        // The first line runs to its end and the last starts at its beginning,
        // which is what makes a multi-line selection a passage rather than a
        // column.
        (row > r0 || col >= c0) && (row < r1 || col <= c1)
    }
}

/// Selecting, in one pane.
#[derive(Debug, Clone)]
pub struct Mode {
    /// The pane being read. Kept so that the mode ends rather than following
    /// you if focus moves.
    pub pane: PaneId,
    /// Where the selection started, once it has.
    pub anchor: Option<(u16, u16)>,
    /// Where it is now, which is also the cursor while selecting with keys.
    pub at: (u16, u16),
    /// Whether the pointer is still down.
    pub dragging: bool,
}

impl Mode {
    pub fn new(pane: PaneId, at: (u16, u16)) -> Self {
        Mode {
            pane,
            anchor: None,
            at,
            dragging: false,
        }
    }

    pub fn span(&self) -> Option<Span> {
        self.anchor.map(|from| Span { from, to: self.at })
    }
}

/// Read a span out of a screen, as text.
///
/// Trailing blanks go, because a terminal pads every line to its full width and
/// nobody wants eighty columns of spaces after a filename. Everything else is
/// left exactly as it was drawn.
pub fn text(screen: &vt100::Screen, span: Span) -> String {
    let (rows, cols) = screen.size();
    let ((r0, c0), (r1, c1)) = span.ordered();
    let mut out = String::new();
    for row in r0..=r1.min(rows.saturating_sub(1)) {
        let from = if row == r0 { c0 } else { 0 };
        let to = if row == r1 {
            c1
        } else {
            cols.saturating_sub(1)
        };
        let mut line = String::new();
        for col in from..=to.min(cols.saturating_sub(1)) {
            if let Some(cell) = screen.cell(row, col) {
                let text = cell.contents();
                match text.is_empty() {
                    // A cell that has never been written is a space, and one
                    // holding the tail of a wide character is nothing at all.
                    true if !cell.is_wide_continuation() => line.push(' '),
                    true => {}
                    false => line.push_str(text.as_ref()),
                }
            }
        }
        out.push_str(line.trim_end());
        if row < r1 {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_dragged_backwards_reads_the_same_way_round() {
        let down = Span {
            from: (1, 2),
            to: (3, 4),
        };
        let up = Span {
            from: (3, 4),
            to: (1, 2),
        };
        assert_eq!(down.ordered(), up.ordered());
    }

    #[test]
    fn a_selection_over_several_lines_is_a_passage_not_a_column() {
        // The gesture people make is "from here to there in the text", and a
        // rectangle would take the same eight columns out of every line.
        let span = Span {
            from: (1, 5),
            to: (3, 2),
        };
        assert!(!span.holds(1, 4), "before the start of the first line");
        assert!(span.holds(1, 5), "the start itself");
        assert!(span.holds(1, 79), "the first line runs to its end");
        assert!(span.holds(2, 0), "a middle line is whole");
        assert!(span.holds(2, 79), "all of it");
        assert!(span.holds(3, 2), "the end itself");
        assert!(!span.holds(3, 3), "past the end of the last line");
        assert!(!span.holds(4, 0), "past the last line");
    }

    #[test]
    fn one_line_selects_only_between_its_ends() {
        let span = Span {
            from: (2, 3),
            to: (2, 6),
        };
        assert!(!span.holds(2, 2));
        assert!(span.holds(2, 3) && span.holds(2, 6));
        assert!(!span.holds(2, 7));
        assert!(!span.holds(1, 4) && !span.holds(3, 4));
    }

    #[test]
    fn copied_lines_do_not_carry_the_terminal_s_padding() {
        // Every line on a terminal is padded to the full width. Copying a
        // filename should give you a filename, not a filename and seventy
        // spaces.
        let mut parser = vt100::Parser::new(4, 20, 0);
        parser.process(b"src/main.rs\r\nsrc/api.rs\r\n");
        let span = Span {
            from: (0, 0),
            to: (1, 19),
        };
        assert_eq!(text(parser.screen(), span), "src/main.rs\nsrc/api.rs");
    }
}
