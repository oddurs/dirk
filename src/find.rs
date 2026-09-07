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

//! Finding the line a build printed four minutes ago.
//!
//! ## A snapshot, not a live query
//!
//! Everything a pane has said is read once, when the search opens, and every
//! keystroke after that filters what was read. The alternative — asking the
//! panes again on each keystroke — means walking vt100's window over five
//! thousand lines per pane per character typed, on the thread that draws.
//!
//! Taking a copy costs a moment at the start and makes the rest free, and it is
//! also the more honest answer: a search that shifted under you as a build
//! printed would be one you could not read the results of.
//!
//! ## Every pane, not just this one
//!
//! The line you are looking for is usually in the pane you were not watching.
//! Results say which pane they came from for that reason, and going to one
//! takes you there.

use crate::mux::PaneId;

/// One line of what some pane has said.
pub struct Line {
    pub pane: PaneId,
    /// What to call the pane it came from, in a list of results.
    pub where_from: String,
    /// How far back this line is in that pane: what `scroll` has to be for it
    /// to be on screen.
    pub back: usize,
    /// Which row of the screen it lands on at that scroll position.
    pub row: u16,
    pub text: String,
}

/// One match, as a place rather than as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub line: usize,
    pub col: u16,
    pub len: u16,
}

pub struct Find {
    pub query: String,
    /// Everything there was to search, as of when this opened.
    lines: Vec<Line>,
    hits: Vec<Hit>,
    pub at: usize,
    /// Which pane was focused when this opened, and how far back it was being
    /// read. Leaving puts both back: a search you abandoned should cost you
    /// nothing, including your place.
    pub origin: (PaneId, usize),
}

impl Find {
    pub fn new(lines: Vec<Line>, origin: (PaneId, usize)) -> Self {
        Find {
            query: String::new(),
            lines,
            hits: Vec::new(),
            at: 0,
            origin,
        }
    }

    /// Work out the matches again, after the query changed.
    ///
    /// Case-insensitive, because nobody typing into a search box is making a
    /// statement about capitals. Substring rather than a pattern: this runs
    /// over every line of every pane on each keystroke, and a regular
    /// expression somebody typed is a way to make that stop returning.
    pub fn refresh(&mut self) {
        self.hits.clear();
        self.at = 0;
        if self.query.is_empty() {
            return;
        }
        let needle = self.query.to_lowercase();
        for (i, line) in self.lines.iter().enumerate() {
            let hay = line.text.to_lowercase();
            // Every occurrence, not just the first: a line that mentions the
            // thing twice is two places you might have meant.
            let mut from = 0;
            while let Some(at) = hay[from..].find(&needle) {
                let start = from + at;
                // Byte offsets from the search, columns for the screen. They
                // differ the moment anything is not ASCII, and the difference
                // is where the highlight lands.
                let col = hay[..start].chars().count() as u16;
                self.hits.push(Hit {
                    line: i,
                    col,
                    len: self.query.chars().count() as u16,
                });
                from = start + needle.len().max(1);
                if from >= hay.len() {
                    break;
                }
            }
        }
    }

    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    pub fn line(&self, hit: Hit) -> Option<&Line> {
        self.lines.get(hit.line)
    }

    /// The match currently pointed at.
    pub fn current(&self) -> Option<(Hit, &Line)> {
        let hit = *self.hits.get(self.at)?;
        Some((hit, self.lines.get(hit.line)?))
    }

    /// Move through the matches, wrapping. A search that stopped at the end
    /// would make you retype it to check the one you scrolled past.
    pub fn step(&mut self, delta: isize) {
        if self.hits.is_empty() {
            return;
        }
        let n = self.hits.len() as isize;
        self.at = (self.at as isize + delta).rem_euclid(n) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(texts: &[&str]) -> Vec<Line> {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| Line {
                pane: 1,
                where_from: "w1:p1".into(),
                back: i,
                row: 0,
                text: (*t).to_string(),
            })
            .collect()
    }

    fn find(texts: &[&str], query: &str) -> Find {
        let mut f = Find::new(lines(texts), (1, 0));
        f.query = query.into();
        f.refresh();
        f
    }

    #[test]
    fn capitals_are_not_a_statement_about_anything() {
        let f = find(&["Error: nothing", "error: something"], "ERROR");
        assert_eq!(f.hits().len(), 2);
    }

    #[test]
    fn a_line_that_says_it_twice_is_two_places() {
        let f = find(&["warn warn"], "warn");
        assert_eq!(f.hits().len(), 2);
        assert_eq!(f.hits()[0].col, 0);
        assert_eq!(f.hits()[1].col, 5);
    }

    #[test]
    fn a_match_is_found_in_columns_and_not_in_bytes() {
        // A line with anything outside ASCII in front of the match: the byte
        // offset and the column stop agreeing, and the column is where the
        // highlight has to go.
        let f = find(&["読み込み error"], "error");
        assert_eq!(f.hits().len(), 1);
        assert_eq!(
            f.hits()[0].col,
            5,
            "the highlight would have landed twelve columns late"
        );
    }

    #[test]
    fn stepping_wraps_rather_than_stopping() {
        // Otherwise checking the one you scrolled past means retyping it.
        let mut f = find(&["a", "a", "a"], "a");
        assert_eq!(f.at, 0);
        f.step(-1);
        assert_eq!(f.at, 2, "back from the first is the last");
        f.step(1);
        assert_eq!(f.at, 0);
    }

    #[test]
    fn an_empty_query_matches_nothing_rather_than_everything() {
        // Every line of every pane is not a result, and offering it as one is
        // how a search box scrolls a thousand rows before you have typed.
        let f = find(&["a", "b"], "");
        assert!(f.hits().is_empty());
    }
}
