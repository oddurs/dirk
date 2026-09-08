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

//! Gotham, as semantic tokens.
//!
//! Lifted from smali, where these values were pinned to match herdr's chrome.
//! In dirk there is nothing to match: dirk draws the chrome, so this file is
//! the definition rather than a copy of one. Nothing outside this module names
//! a colour; call sites ask for the job a colour does.
//!
//! The one idea worth stating: **chrome is not content.** The sidebar and rail
//! paint their own ground. Panes paint nothing of their own — they carry
//! whatever the program inside them drew. That difference is the whole reason
//! the nav reads as nav.

// The palette is complete on purpose: a token defined but not yet used is a
// decision already made, not dead weight.
#![allow(dead_code)]

use ratatui::style::{Color, Modifier, Style};

/// Gotham base ramp, darkest to lightest.
const BASE0: Color = Color::Rgb(0x0a, 0x0f, 0x14); // panel ground
const BASE3: Color = Color::Rgb(0x09, 0x1f, 0x2e); // active row
const BASE4: Color = Color::Rgb(0x0a, 0x37, 0x49); // selection
const BASE5: Color = Color::Rgb(0x24, 0x53, 0x61); // rules, inactive glyphs
const BASE6: Color = Color::Rgb(0x59, 0x9c, 0xaa); // secondary text
const BASE8: Color = Color::Rgb(0xd3, 0xeb, 0xe9); // primary text

const ACCENT: Color = Color::Rgb(0x33, 0x85, 0x9d);
const RED: Color = Color::Rgb(0xc3, 0x30, 0x27);
const GREEN: Color = Color::Rgb(0x26, 0xa9, 0x8b);
const YELLOW: Color = Color::Rgb(0xed, 0xb5, 0x4b);
const PEACH: Color = Color::Rgb(0xd2, 0x69, 0x39);
const MAUVE: Color = Color::Rgb(0x88, 0x8b, 0xa5);

/// The rail sits one step off the panel ground so the bottom band reads as a
/// separate surface without a border doing the work.
const RAIL: Color = Color::Rgb(0x0c, 0x1a, 0x24);

#[derive(Debug, Clone, Copy)]
pub struct Theme;

impl Theme {
    // ── Surfaces ────────────────────────────────────────────────────────
    pub fn panel(self) -> Style {
        Style::default().bg(BASE0).fg(BASE8)
    }
    pub fn rail(self) -> Style {
        Style::default().bg(RAIL).fg(BASE8)
    }
    pub fn active_row(self) -> Style {
        Style::default().bg(BASE3)
    }
    pub fn selected(self) -> Style {
        Style::default().bg(BASE4).fg(BASE8)
    }

    // ── Chrome: structure, never data ───────────────────────────────────
    /// Panel titles. Rendered upper-case with letter-spacing by `chrome`.
    pub fn title(self) -> Style {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    }
    /// Rules and tree spines. Bright enough to follow across a panel, dim
    /// enough that a column of them is not the first thing you see.
    pub fn rule_strong(self) -> Style {
        Style::default().fg(BASE5)
    }
    pub fn text(self) -> Style {
        Style::default().fg(BASE8)
    }
    pub fn dim(self) -> Style {
        Style::default().fg(BASE6)
    }
    pub fn faint(self) -> Style {
        Style::default().fg(BASE5)
    }
    /// Keys in a hint line — the letter you press, not the word describing it.
    pub fn key(self) -> Style {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    }

    // ── Identity: which thing this is ───────────────────────────────────
    pub fn project(self) -> Style {
        Style::default().fg(BASE8).add_modifier(Modifier::BOLD)
    }
    pub fn branch(self) -> Style {
        Style::default().fg(MAUVE)
    }
    pub fn worktree(self) -> Style {
        Style::default().fg(PEACH)
    }
    pub fn intent(self) -> Style {
        Style::default().fg(BASE6)
    }
    pub fn number(self) -> Style {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    }

    // ── Status: a state, and reserved for it ────────────────────────────
    pub fn ok(self) -> Style {
        Style::default().fg(GREEN)
    }
    pub fn warn(self) -> Style {
        Style::default().fg(YELLOW)
    }
    pub fn critical(self) -> Style {
        Style::default().fg(RED)
    }
    pub fn working(self) -> Style {
        Style::default().fg(ACCENT)
    }
    pub fn idle(self) -> Style {
        Style::default().fg(BASE5)
    }

    /// The colour and glyph for one herdr agent state.
    ///
    /// `blocked` is the only one that gets red: it is the only state that is
    /// waiting on the human. `done` is green because it is finished work the
    /// human has not looked at; `idle` is grey because nothing is owed.
    ///
    /// `unknown` draws a space, not a `?`. herdr reports it both for an agent
    /// it cannot classify and for a pane holding no agent at all, and the
    /// second case is the common one — a column of question marks down a tree
    /// of ordinary shells is noise claiming to be information. The space keeps
    /// the column aligned, which is the only thing the glyph was doing there.
    /// The colour a state is drawn in.
    ///
    /// The mark it is drawn with lives in `glyph.rs`, because it is one of a
    /// set and this is not the only place that draws it -- the rail wrote its
    /// own `+` and `!` for the attention counts, and nothing would have noticed
    /// if these had changed and those had not.
    pub fn state_style(self, state: &str) -> Style {
        match state {
            "blocked" => self.critical().add_modifier(Modifier::BOLD),
            "working" => self.working(),
            "done" => self.ok(),
            "idle" | "starting" => self.idle(),
            _ => self.faint(),
        }
    }

    /// The colour for a cairn item's priority field.
    pub fn priority(self, p: &str) -> Style {
        match p {
            "p0" => self.critical(),
            "p1" => self.warn(),
            "p2" => self.dim(),
            _ => self.faint(),
        }
    }
}

pub const THEME: Theme = Theme;
