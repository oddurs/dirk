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
    /// A search in this pane, once there is one.
    pub search: Option<Search>,
}

/// Looking for something in the pane you are already reading.
///
/// Session-wide find is the right default and better than what tmux does: the
/// line you want is usually in the pane you were *not* watching. It is not what
/// you want once you are in copy mode looking at one pane's scrollback — the
/// results take you away, and the answer arrives as a list of places rather
/// than as a cursor two hundred lines up from where you are.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// What has been typed so far.
    pub query: String,
    /// Whether `n` goes forward. `?` searches backward and `N` inverts it,
    /// which is what the pair does in every pager.
    pub forward: bool,
    /// Whether the query is still being typed.
    pub typing: bool,
}

impl Search {
    /// Smart case: case-insensitive unless the query has an uppercase letter in
    /// it, which is the convention every tool in this space already uses.
    pub fn matches(&self, line: &str) -> Option<usize> {
        if self.query.is_empty() {
            return None;
        }
        match self.query.chars().any(char::is_uppercase) {
            true => line.find(&self.query),
            false => line.to_lowercase().find(&self.query.to_lowercase()),
        }
    }

    /// The next line and column matching, from `from`, in `lines`.
    ///
    /// Wraps, because a search that stopped at the end of the screen would be a
    /// search you had to know the shape of. `None` when nothing matches at all.
    pub fn find(&self, lines: &[String], from: (u16, u16), forward: bool) -> Option<(u16, u16)> {
        let n = lines.len();
        if n == 0 {
            return None;
        }
        let start = (from.0 as usize).min(n - 1);
        for step in 1..=n {
            let i = match forward {
                true => (start + step) % n,
                false => (start + n - step % n) % n,
            };
            if let Some(col) = self.matches(&lines[i]) {
                return Some((i as u16, col as u16));
            }
        }
        // Nowhere else; the line under the cursor is the only candidate left.
        self.matches(&lines[start])
            .map(|c| (start as u16, c as u16))
    }
}

impl Mode {
    pub fn new(pane: PaneId, at: (u16, u16)) -> Self {
        Mode {
            pane,
            anchor: None,
            at,
            dragging: false,
            search: None,
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

/// Where a word motion lands, on one screen.
///
/// vi's rules, not a Unicode segmentation library's: what somebody pressing `w`
/// expects here is what vi does, including the places vi is arguably wrong. A
/// word is a run of word characters or a run of punctuation; a big word is a
/// run of anything that is not a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Word {
    /// `w` — the start of the next word.
    Next,
    /// `b` — the start of this word, or of the one before it.
    Back,
    /// `e` — the end of this word, or of the next.
    End,
}

/// Which class a character belongs to, for `w` and `b`.
///
/// Three classes and not two, because vi stops between a word and the
/// punctuation beside it: `foo.bar` is three words, which is what makes `w`
/// useful in code rather than in prose.
fn class(c: char, big: bool) -> u8 {
    match c {
        c if c.is_whitespace() => 0,
        _ if big => 1,
        c if c.is_alphanumeric() || c == '_' => 1,
        _ => 2,
    }
}

/// Move by a word within `line`, from `col`.
///
/// Answers a column on the same line. Running off the end is the end: crossing
/// lines would need the scrollback, and the caller is already the thing that
/// knows how to move between rows.
pub fn word(line: &str, col: u16, motion: Word, big: bool) -> u16 {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return 0;
    }
    let last = chars.len() - 1;
    let at = (col as usize).min(last);
    let cls = |i: usize| class(chars[i], big);

    let landed = match motion {
        Word::Next => {
            let mut i = at;
            // Off the end of what is under the cursor, then over the gap.
            let start = cls(i);
            while i < last && cls(i) == start && start != 0 {
                i += 1;
            }
            while i < last && cls(i) == 0 {
                i += 1;
            }
            i
        }
        Word::Back => {
            let mut i = at;
            while i > 0 && cls(i - 1) == 0 {
                i -= 1;
            }
            // Then to the start of whatever that is.
            if i > 0 {
                i -= 1;
                let here = cls(i);
                while i > 0 && cls(i - 1) == here && here != 0 {
                    i -= 1;
                }
            }
            i
        }
        Word::End => {
            let mut i = at;
            // Past the end of this one, so pressing it twice moves twice.
            if i < last {
                i += 1;
            }
            while i < last && cls(i) == 0 {
                i += 1;
            }
            let here = cls(i);
            while i < last && cls(i + 1) == here && here != 0 {
                i += 1;
            }
            i
        }
    };
    landed as u16
}

/// The row a paragraph motion lands on.
///
/// A paragraph is a run of lines with something on them; a blank line is the
/// boundary. `{` goes back to one and `}` forward, which in a terminal is how
/// you move between one command's output and the next.
pub fn paragraph(lines: &[String], row: u16, forward: bool) -> u16 {
    let last = lines.len().saturating_sub(1);
    let blank = |i: usize| lines.get(i).is_none_or(|l| l.trim().is_empty());
    let mut i = (row as usize).min(last);
    let step = |i: usize| match forward {
        true => (i + 1).min(last),
        false => i.saturating_sub(1),
    };
    // Off whatever boundary we are standing on, so pressing it twice moves
    // twice rather than staying put.
    while i != step(i) && blank(i) {
        i = step(i);
    }
    while i != step(i) && !blank(step(i)) {
        i = step(i);
    }
    match i == step(i) {
        true => i as u16,
        false => step(i) as u16,
    }
}

#[cfg(test)]
mod motions {
    use super::*;

    fn lines(of: &[&str]) -> Vec<String> {
        of.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_search_is_case_insensitive_until_you_type_a_capital() {
        // The convention every tool in this space already uses, so nobody has
        // to be told about it.
        let lower = Search {
            query: "err".into(),
            forward: true,
            typing: false,
        };
        assert!(lower.matches("ERROR: nope").is_some());
        let upper = Search {
            query: "ERR".into(),
            ..lower.clone()
        };
        assert!(upper.matches("ERROR: nope").is_some());
        assert!(upper.matches("error: nope").is_none());
    }

    #[test]
    fn a_search_wraps_rather_than_stopping_at_the_edge() {
        // A search that stopped at the end of the screen is one you would have
        // to know the shape of.
        let text = lines(&["alpha", "beta", "gamma", "beta"]);
        let s = Search {
            query: "beta".into(),
            forward: true,
            typing: false,
        };
        assert_eq!(s.find(&text, (0, 0), true), Some((1, 0)));
        assert_eq!(s.find(&text, (1, 0), true), Some((3, 0)));
        assert_eq!(s.find(&text, (3, 0), true), Some((1, 0)), "wrapped");
        assert_eq!(s.find(&text, (3, 0), false), Some((1, 0)));
        assert_eq!(s.find(&text, (1, 0), false), Some((3, 0)), "wrapped back");
    }

    #[test]
    fn a_search_that_matches_nothing_says_so_rather_than_moving() {
        let text = lines(&["alpha", "beta"]);
        let s = Search {
            query: "zzmissing".into(),
            forward: true,
            typing: false,
        };
        assert_eq!(s.find(&text, (0, 0), true), None);
        // An empty query matches nothing rather than everything, so backspacing
        // to nothing does not jump you to the top.
        let empty = Search {
            query: String::new(),
            ..s
        };
        assert_eq!(empty.find(&text, (0, 0), true), None);
    }

    #[test]
    fn a_word_is_what_vi_means_by_one() {
        // Three classes, not two: `foo.bar` is three words, which is what makes
        // this useful in code rather than in prose.
        let line = "foo.bar  baz";
        assert_eq!(word(line, 0, Word::Next, false), 3, "to the dot");
        assert_eq!(word(line, 3, Word::Next, false), 4, "to bar");
        assert_eq!(word(line, 4, Word::Next, false), 9, "over the gap to baz");
        assert_eq!(word(line, 9, Word::Back, false), 4);
        assert_eq!(word(line, 4, Word::Back, false), 3);
        assert_eq!(word(line, 0, Word::End, false), 2, "the end of foo");
        assert_eq!(word(line, 2, Word::End, false), 3, "the dot is a word");
    }

    #[test]
    fn a_big_word_is_anything_that_is_not_a_space() {
        let line = "foo.bar  baz";
        assert_eq!(word(line, 0, Word::Next, true), 9, "straight over the dot");
        assert_eq!(word(line, 9, Word::Back, true), 0);
        assert_eq!(word(line, 0, Word::End, true), 6, "the end of foo.bar");
    }

    #[test]
    fn a_motion_at_the_edge_stays_on_the_line() {
        // Crossing lines would need the scrollback, and the row is the caller's
        // business. Standing still is the honest answer.
        assert_eq!(word("abc", 2, Word::Next, false), 2);
        assert_eq!(word("abc", 0, Word::Back, false), 0);
        assert_eq!(word("", 0, Word::Next, false), 0);
    }

    #[test]
    fn a_paragraph_is_a_run_of_lines_with_something_on_them() {
        let lines: Vec<String> = ["one", "two", "", "three", "", "", "four"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(paragraph(&lines, 0, true), 2, "to the blank after one-two");
        assert_eq!(paragraph(&lines, 2, true), 4, "to the blank after three");
        assert_eq!(paragraph(&lines, 6, false), 5, "back to the blank run");
        // Pressing it at the end does not wrap, and pressing it twice from a
        // boundary moves twice rather than staying put.
        assert_eq!(paragraph(&lines, 6, true), 6);
        assert_eq!(paragraph(&lines, 0, false), 0);
    }
}
