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

//! Every mark the interface draws, in one place.
//!
//! They were literals scattered between the theme, the nav and the rail, which
//! meant two of them had drifted: the rail wrote its own `+` and `!` for the
//! attention counts while the nav took its from the theme, and nothing would
//! have noticed if one changed.
//!
//! ## A glyph is a width before it is a picture
//!
//! The nav's arithmetic is exact — a name is elided to what is left after the
//! age, and the age is reserved before the name is written. A mark that takes
//! two columns where one was budgeted does not look slightly wrong, it shifts
//! every column after it on that row.
//!
//! So width is part of the table rather than a hope. `unicode-width` answers
//! for text dirk did not choose (a workspace named in Japanese, an emoji an
//! agent put in its own title), and the table answers for the marks dirk did.
//!
//! ## Why `unicode` is still the default
//!
//! There are two full sets and `round` is arguably the prettier: `○ ◐ ● ✓` is
//! a progression, and four marks that are a progression are four marks you can
//! read as one thing rather than as four.
//!
//! It is not the default, and that is a decision rather than an oversight.
//! `! * + ·` are legible to somebody who has never seen dirk and never read
//! anything about it — an exclamation mark means attention wherever you have
//! seen one before. A circle three-quarters filled does not mean anything
//! until you have been told what the other quarters mean. And `✓` for `done`
//! competes with the tick a test suite prints, which is the one thing in a
//! pane it is most likely to be sitting next to.
//!
//! So: shipped, named, one line to choose, and not chosen for you.
//!
//! ## Why there is no Nerd Font set
//!
//! It was considered and rejected, and the reasons are worth keeping:
//!
//! 1. **A state mark has to be readable at one cell without being learned.**
//!    `!` is attention and `+` is complete to anyone who has used a computer.
//!    An icon at one cell is a shape you have to be taught, and these are the
//!    most important four characters in the interface.
//! 2. **Nerd Font glyphs live in the Private Use Area**, where Unicode assigns
//!    no width, so terminals disagree — and disagreeing about width does not
//!    degrade here, it breaks the column.
//! 3. **dirk cannot check that the font is installed.** Shipping a set that
//!    renders as tofu for anyone who has not configured their terminal is a
//!    default that fails.
//!
//! What someone with the font can do is say so, per mark, with the width they
//! know their terminal gives it. That is the mechanism without dirk owning
//! either the maintenance or the breakage.

use unicode_width::UnicodeWidthStr;

/// One mark, and how many columns the terminal gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub text: String,
    pub cells: u16,
}

macro_rules! marks {
    ($($variant:ident $name:literal $ascii:literal $unicode:literal),* $(,)?) => {
        /// Everything the interface draws that is not text.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum G { $($variant),* }

        impl G {
            pub const ALL: &'static [G] = &[$(G::$variant),*];

            /// The name this mark answers to in a configuration file.
            pub fn name(self) -> &'static str {
                match self { $(G::$variant => $name),* }
            }

            fn ascii(self) -> &'static str {
                match self { $(G::$variant => $ascii),* }
            }

            fn unicode(self) -> &'static str {
                match self { $(G::$variant => $unicode),* }
            }
        }
    };
}

marks! {
    // The four that matter most, and the two that keep them company.
    Blocked   "blocked"    "!"  "!",
    Done      "done"       "+"  "+",
    Working   "working"    "*"  "*",
    Idle      "idle"       "."  "·",
    Starting  "starting"   "o"  "◦",
    Unknown   "unknown"    " "  " ",

    // What a workspace is, beside its name.
    Worktree  "worktree"   "y"  "⑂",
    Zoomed    "zoomed"     "Z"  "⤢",
    Held      "held"       "."  "·",

    // How far a branch has gone, and how far it has been left.
    Ahead     "ahead"      "^"  "↑",
    Behind    "behind"     "v"  "↓",

    // Disclosure, and the pane subtree under a workspace.
    Collapsed "collapsed"  ">"  "▸",
    Rule      "rule"       "-"  "─",
    Expanded  "expanded"   "v"  "▾",
    TreeMid   "tree-mid"   "+"  "├",
    TreeLast  "tree-last"  "\\" "└",

    // A layout with panes behind it, as against one not built yet.
    Running   "running"    "o"  "•",

    // The rail: a chip you are in, a chip you are not, and the way out.
    BarFocused "bar-focused" "|" "▊",
    BarPlain   "bar-plain"   ":" "▏",
    Close      "close"       "x" "✕",

    // Where a name ran out of room, what stands between two things on one
    // line, and what a board says when it could not answer.
    Ellipsis  "ellipsis"   "~"  "…",
    Absent    "absent"     "-"  "—",
    Sep       "separator"  "-"  "·",
    Enter     "enter"      "^M" "↵",

    // The default mark before the brand name. A user who names their own is
    // naming content and this stops applying to them.
    Brand     "brand"      "#"  "◆",
}

impl G {
    /// What the `round` set draws instead, where it differs from `unicode`.
    ///
    /// A diff rather than a third column in the table above. The set exists
    /// because four marks read better as circles; writing the other twenty out
    /// again as "the same" would make every new mark a three-way decision that
    /// is really a one-way one, and the day somebody forgot the third column
    /// the set would silently lose a glyph.
    ///
    /// A progression rather than four unrelated pictures: empty at rest, half
    /// filled while working, filled when it wants you, and a tick when it is
    /// finished. `starting` is the dotted one, which is the circle not drawn
    /// yet.
    fn round(self) -> Option<&'static str> {
        Some(match self {
            G::Idle => "○",
            G::Working => "◐",
            G::Blocked => "●",
            G::Done => "✓",
            G::Starting => "◌",
            _ => return None,
        })
    }

    /// The mark for an agent state, by the name the theme and the nav share.
    pub fn state(name: &str) -> G {
        match name {
            "blocked" => G::Blocked,
            "done" => G::Done,
            "working" => G::Working,
            "idle" => G::Idle,
            "starting" => G::Starting,
            _ => G::Unknown,
        }
    }

    pub fn named(name: &str) -> Option<G> {
        G::ALL.iter().copied().find(|g| g.name() == name)
    }
}

/// A resolved set: one of the shipped ones, with whatever was overridden.
#[derive(Debug, Clone)]
pub struct Glyphs(Vec<Mark>);

impl Default for Glyphs {
    fn default() -> Self {
        Glyphs::set("unicode")
    }
}

impl Glyphs {
    /// The sets dirk ships, as the names a configuration file may use.
    ///
    /// Here rather than in `config.rs` so the two cannot drift: a name this
    /// list does not have is a name `set` would quietly answer `unicode` to.
    pub const SETS: &'static [&'static str] = &["unicode", "ascii", "round"];

    /// A shipped set by name. Anything unrecognised is `unicode`, which is what
    /// dirk has always drawn — reported by `config::problems` rather than
    /// guessed at here.
    pub fn set(name: &str) -> Self {
        Glyphs(
            G::ALL
                .iter()
                .map(|g| {
                    let text = match name {
                        "ascii" => g.ascii(),
                        // The round set is the unicode one with the states
                        // redrawn, so anything it does not name falls through.
                        "round" => g.round().unwrap_or_else(|| g.unicode()),
                        _ => g.unicode(),
                    }
                    .to_string();
                    // Measured, not assumed. A shipped set is ordinary text
                    // and `unicode-width` is the authority on it -- and it
                    // caught `^M`, the ascii mark for enter, being budgeted at
                    // one column for as long as there has been an ascii set.
                    // The override path still takes a declared width, because
                    // that is the case where nothing here can measure.
                    let cells = text.width().max(1) as u16;
                    Mark { text, cells }
                })
                .collect(),
        )
    }

    /// Replace one mark. The width comes from the caller because dirk cannot
    /// measure it: the terminal knows, the font knows, and this process has no
    /// way to ask either.
    pub fn set_one(&mut self, g: G, text: &str, cells: u16) {
        if let Some(slot) = self.0.get_mut(g as usize) {
            *slot = Mark {
                text: text.to_string(),
                cells: cells.max(1),
            };
        }
    }

    pub fn text(&self, g: G) -> &str {
        self.0.get(g as usize).map_or(" ", |m| m.text.as_str())
    }

    pub fn cells(&self, g: G) -> u16 {
        self.0.get(g as usize).map_or(1, |m| m.cells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_has_a_name_that_finds_it_again() {
        // The names are the configuration surface. One that does not round-trip
        // is a mark nobody can override and a typo nobody can be told about.
        for &g in G::ALL {
            assert_eq!(
                G::named(g.name()),
                Some(g),
                "{} did not round-trip",
                g.name()
            );
        }
    }

    #[test]
    fn the_ascii_set_is_ascii() {
        // The whole point of it. A set that reaches for U+2026 because the
        // ellipsis looked lonely is a set that does not solve the problem it
        // exists for.
        let g = Glyphs::set("ascii");
        for &mark in G::ALL {
            let text = g.text(mark);
            assert!(
                text.is_ascii(),
                "{}: {text:?} is not ascii in the ascii set",
                mark.name()
            );
        }
    }

    #[test]
    fn a_shipped_set_is_budgeted_at_the_width_it_actually_draws() {
        // The nav's arithmetic is exact: a mark that takes two columns where
        // one was budgeted does not look slightly wrong, it shifts every
        // column after it on that row.
        for name in Glyphs::SETS {
            let g = Glyphs::set(name);
            for &mark in G::ALL {
                let text = g.text(mark);
                assert_eq!(
                    g.cells(mark) as usize,
                    text.width().max(1),
                    "{name}: {} draws {text:?} and was budgeted {} columns",
                    mark.name(),
                    g.cells(mark)
                );
            }
        }
    }

    #[test]
    fn every_state_mark_is_one_cell_in_every_set() {
        // These live in the tightest column there is, between the disclosure
        // and the number. A two-column state mark would shift the number, the
        // name and the age on that row and on no other.
        for name in Glyphs::SETS {
            let g = Glyphs::set(name);
            for state in ["blocked", "done", "working", "idle", "starting", "unknown"] {
                let mark = G::state(state);
                assert_eq!(
                    g.cells(mark),
                    1,
                    "{name}: {state} is {:?}, which is not one column",
                    g.text(mark)
                );
            }
        }
    }

    #[test]
    fn the_round_set_redraws_the_states_and_nothing_else() {
        // It is the unicode set with four marks changed. If it started
        // diverging elsewhere it would be a second interface to maintain
        // rather than a second opinion about four glyphs.
        let round = Glyphs::set("round");
        let plain = Glyphs::default();
        let changed: Vec<_> = G::ALL
            .iter()
            .filter(|&&g| round.text(g) != plain.text(g))
            .map(|g| g.name())
            .collect();
        assert_eq!(
            changed,
            ["blocked", "done", "working", "idle", "starting"],
            "the round set drifted from the unicode one"
        );
        assert_eq!(round.text(G::Idle), "○");
        assert_eq!(round.text(G::Working), "◐");
        assert_eq!(round.text(G::Done), "✓");
    }

    #[test]
    fn a_set_nobody_ships_is_a_problem_rather_than_a_shrug() {
        // `set` answers the default for a name it does not know, so without
        // this a typo is a configuration that appears to have been ignored.
        let mut cfg = crate::config::Config::default();
        cfg.nav.glyphs = "circles".into();
        let said = crate::config::complaints(&cfg).join("\n");
        assert!(
            said.contains("circles") && said.contains("round"),
            "an unknown set was not reported against the sets that exist: {said:?}"
        );
        for name in Glyphs::SETS {
            let mut cfg = crate::config::Config::default();
            cfg.nav.glyphs = (*name).into();
            assert!(
                !crate::config::complaints(&cfg)
                    .iter()
                    .any(|p| p.contains("glyph set")),
                "{name} is shipped and was refused"
            );
        }
    }

    #[test]
    fn the_unicode_set_is_what_dirk_has_always_drawn() {
        let g = Glyphs::default();
        assert_eq!(g.text(G::Blocked), "!");
        assert_eq!(g.text(G::Done), "+");
        assert_eq!(g.text(G::Working), "*");
        assert_eq!(g.text(G::Idle), "·");
        assert_eq!(g.text(G::Worktree), "⑂");
        assert_eq!(g.text(G::Expanded), "▾");
    }

    #[test]
    fn an_override_carries_the_width_it_was_given() {
        // The reason overrides exist: someone with a Nerd Font installed knows
        // what their terminal does with it, and dirk does not.
        let mut g = Glyphs::default();
        g.set_one(G::Blocked, "\u{f071}", 2);
        assert_eq!(g.text(G::Blocked), "\u{f071}");
        assert_eq!(g.cells(G::Blocked), 2);
        // Zero would make a row that never advances.
        g.set_one(G::Done, "x", 0);
        assert_eq!(g.cells(G::Done), 1);
    }
}
