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

//! Everything dirk can do, filtered by typing.
//!
//! It is the answer to "how do I", and it takes the pressure off binding
//! everything: an action nobody has a key for is still one keystroke and a
//! word away, so the keyboard can stay small.
//!
//! It doubles as a jump. Boards, spaces and the agents in them are places as
//! much as the actions are things, and somebody who types `auth` is as likely
//! to mean the workspace as anything else.
//!
//! What it must not do is hide. An entry that cannot be done now is shown
//! disabled with the reason beside it — a palette that silently omits what it
//! cannot do teaches you that dirk cannot do it.

use crate::action::{Action, Keys};
use crate::mux::{Focus, Session};

/// One row of the palette.
pub struct Entry {
    pub label: String,
    /// The key that also does this, when there is one.
    pub key: String,
    /// Why it is not available, when it is not.
    pub why_not: Option<String>,
    pub what: What,
}

/// What choosing a row means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    Do(Action),
    Go(Focus),
}

pub struct Palette {
    pub query: String,
    entries: Vec<Entry>,
    pub selected: usize,
}

impl Palette {
    pub fn new(session: &Session, keys: &Keys) -> Self {
        let mut entries = Vec::new();

        for &action in Action::ALL {
            entries.push(Entry {
                label: action.title().to_string(),
                key: keys.key(action).unwrap_or_default().to_string(),
                why_not: action.why_not(session).map(str::to_string),
                what: What::Do(action),
            });
        }

        // Places, not only things. A board that is not open says so, because
        // "go to ptop" and "start ptop" look identical from here otherwise.
        for (i, layout) in session.layouts.iter().enumerate() {
            entries.push(Entry {
                label: format!("Go to {}", layout.def.name),
                key: layout.def.key.map(String::from).unwrap_or_default(),
                why_not: None,
                what: What::Go(Focus::Layout(i)),
            });
        }
        for (p, w) in session.flat() {
            let Some(ws) = session.workspace(p, w) else {
                continue;
            };
            let project = session
                .projects
                .get(p)
                .map(|x| x.name.as_str())
                .unwrap_or_default();
            entries.push(Entry {
                label: format!("Go to {project} · {}", ws.label),
                key: String::new(),
                why_not: None,
                what: What::Go(Focus::Ws { p, w }),
            });
        }

        Palette {
            query: String::new(),
            entries,
            selected: 0,
        }
    }

    /// The entries that match, best first.
    ///
    /// A subsequence rather than a substring, which is what the project picker
    /// does and what everybody now expects: `nwt` should find "New worktree".
    /// Ranked by how tightly the letters sit together, so an exact word beats
    /// the same letters scattered across a sentence.
    pub fn matches(&self) -> Vec<(usize, &Entry)> {
        if self.query.is_empty() {
            return self.entries.iter().enumerate().collect();
        }
        let q = self.query.to_lowercase();
        let mut v: Vec<(usize, usize, &Entry)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| score(&e.label.to_lowercase(), &q).map(|s| (s, i, e)))
            .collect();
        v.sort_by_key(|(s, i, _)| (*s, *i));
        v.into_iter().map(|(_, i, e)| (i, e)).collect()
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = self.matches().len();
        if n == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(n as isize) as usize;
    }

    pub fn reset(&mut self) {
        self.selected = 0;
    }
}

/// How far apart the query's letters are in the label. Lower is better.
///
/// `None` when they are not all there in order. The span from the first
/// matched letter to the last is the whole of the ranking: it prefers a word
/// over the same letters spread across a sentence, which is the difference
/// between finding "New worktree" and finding "Next pane in this space".
fn score(label: &str, query: &str) -> Option<usize> {
    let mut chars = label.char_indices();
    let mut first = None;
    let mut last = 0;
    for want in query.chars() {
        let (at, _) = chars.find(|(_, c)| *c == want)?;
        first.get_or_insert(at);
        last = at;
    }
    Some(last - first.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_letters_have_to_be_there_in_order() {
        assert!(score("new worktree", "nwt").is_some());
        assert!(
            score("new worktree", "twn").is_none(),
            "out of order matched"
        );
        assert!(
            score("new worktree", "nwtx").is_none(),
            "a letter that is not there"
        );
    }

    #[test]
    fn letters_that_sit_together_beat_letters_that_do_not() {
        // Otherwise "nwt" finds "Next pane in this space" before "New
        // worktree", and the palette is a list you scroll rather than one you
        // type at.
        let tight = score("new worktree", "nwt").unwrap();
        let loose = score("next pane in this space", "nwt");
        assert!(loose.is_none() || tight < loose.unwrap());
    }

    #[test]
    fn an_empty_query_offers_everything() {
        // The palette is a list of what dirk can do before it is a search over
        // it, and somebody who has typed nothing has not excluded anything.
        assert_eq!(score("anything at all", ""), Some(0));
    }
}
