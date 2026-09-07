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

//! A noise when an agent blocks, and a different one when it finishes.
//!
//! The whole reason to run agents in parallel is to be doing something else
//! while they work, so the state you most need is the one you are not looking
//! at. Two events are worth hearing — entering `blocked` and entering `done` —
//! and they have to be distinguishable with your back to the screen, because
//! that is the point. One interrupts and one satisfies.
//!
//! Everything else here is a rule about when to stay quiet, and those decide
//! whether this is loved or muted within a week. The gating happens where the
//! notification gating already happens, so the two cannot disagree about what
//! just occurred: not while you are looking at it, not more often than the
//! floor, and not at all for a project that asked to be silent.
//!
//! No audio dependency. dirk already shells out for the process table, git,
//! notifications and naming; this is the same kind of thing, and a crate for it
//! is a build cost and a transitive tree for something allowed to do nothing.

use std::process::{Command, Stdio};

/// Which of the two, and the only two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    Blocked,
    Done,
}

impl Alert {
    pub fn name(self) -> &'static str {
        match self {
            Alert::Blocked => "blocked",
            Alert::Done => "done",
        }
    }

    pub fn named(name: &str) -> Option<Alert> {
        match name {
            "blocked" => Some(Alert::Blocked),
            "done" => Some(Alert::Done),
            _ => None,
        }
    }
}

/// Make the noise, on a thread of its own.
///
/// Never blocks and never reports failure, for the same reason a notification
/// does not: it is the least important thing dirk does and it must not be able
/// to hold up a redraw.
pub fn play(cfg: &crate::config::Sound, which: Alert) {
    if !cfg.enabled {
        return;
    }
    let command = cfg.command(which);
    // Nothing to run: the terminal's own bell, written here rather than on a
    // thread of its own. It goes to the same stdout the renderer is writing
    // frames to, and two writers to one terminal is how a bell lands in the
    // middle of an escape sequence.
    if command.is_empty() {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = out.write_all(bell(which));
        let _ = out.flush();
        return;
    }
    std::thread::spawn(move || {
        if let Some((program, args)) = command.split_first() {
            let _ = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    });
}

/// The bell, when there is no player and no file.
///
/// Once for finished and twice for waiting: as much of a distinction as a bell
/// can carry, and enough to tell them apart without looking.
pub fn bell(which: Alert) -> &'static [u8] {
    match which {
        Alert::Blocked => b"\x07\x07",
        Alert::Done => b"\x07",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Sound;

    #[test]
    fn the_two_events_do_not_make_the_same_noise() {
        // With your back to the screen they are all you have. One that
        // interrupts and one that satisfies, and if they sound alike the
        // feature is a bell with extra steps.
        //
        // Where the distinction lives depends on the machine: two files on a
        // mac, and the number of beeps everywhere else. Either is a difference;
        // having neither is the bug.
        let cfg = Sound::default();
        let audible = |a| (cfg.command(a), bell(a));
        assert_ne!(
            audible(Alert::Blocked),
            audible(Alert::Done),
            "blocked and done sound the same by default"
        );
    }

    #[test]
    fn the_names_round_trip() {
        for a in [Alert::Blocked, Alert::Done] {
            assert_eq!(Alert::named(a.name()), Some(a));
        }
        assert_eq!(Alert::named("working"), None, "working is not an event");
    }

    #[test]
    fn a_configured_command_wins_over_the_shipped_one() {
        let cfg = Sound {
            blocked: vec!["mine".into(), "a.wav".into()],
            ..Sound::default()
        };
        assert_eq!(cfg.command(Alert::Blocked), ["mine", "a.wav"]);
    }
}
