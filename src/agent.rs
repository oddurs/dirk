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

//! What is running in a pane.
//!
//! The signal is the tty's **foreground process group** — precisely "what is
//! running here right now", since that is the thing a shell sets before it waits
//! for a command and resets when the command ends. A pane whose foreground group
//! is its own shell is at a prompt; one whose foreground group is something else
//! is running that thing.
//!
//! Turning a process group into a name needs the process table, so this shells
//! out to `ps` — once for the whole session rather than once per pane, and never
//! on the drawing thread.
//!
//! Three answers, not two. A pane holds an agent, or is a shell at its prompt,
//! or is running something else that is neither. Reporting "unknown agent" for a
//! build is how a status column turns into noise.

use std::collections::HashMap;
use std::process::Command;

/// A coding agent dirk can recognise.
///
/// Matched on the basename of the process's `comm`. Deliberately not on its
/// command line: a title is how an agent says what it is *doing*, `comm` is what
/// it *is*, and reading someone else's argv to identify them is guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub name: &'static str,
    names: &'static [&'static str],
}

/// The table. Adding one is a line.
pub const KINDS: &[Kind] = &[
    Kind {
        name: "claude",
        names: &["claude"],
    },
    Kind {
        name: "codex",
        names: &["codex"],
    },
    Kind {
        name: "aider",
        names: &["aider"],
    },
    Kind {
        name: "goose",
        names: &["goose"],
    },
];

/// Shells, so that "at a prompt" is a state rather than an unrecognised
/// program. `agent start` will need to know a pane is free.
const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "nu",
];

/// What a pane is running.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Occupant {
    /// Nothing could be determined — no foreground group, or a process that
    /// vanished between one sample and the next.
    #[default]
    Unknown,
    /// A shell at its prompt. The pane is free.
    Shell,
    /// Something that is neither a shell nor a known agent: a build, an editor,
    /// a system monitor. Named, because the name is worth showing and guessing
    /// past it is not.
    Program(String),
    Agent(Kind),
}

impl Occupant {
    pub fn agent(&self) -> Option<Kind> {
        match self {
            Occupant::Agent(k) => Some(*k),
            _ => None,
        }
    }

    /// True when an agent could be started here.
    pub fn available(&self) -> bool {
        matches!(self, Occupant::Shell)
    }
}

/// Classify one process name. `comm` may be a full path or a bare name
/// depending on the platform and the process, so only the basename is matched.
pub fn classify(comm: &str) -> Occupant {
    let base = comm.rsplit('/').next().unwrap_or(comm).trim();
    if base.is_empty() {
        return Occupant::Unknown;
    }
    // A login shell is "-zsh".
    let base = base.strip_prefix('-').unwrap_or(base);

    if let Some(kind) = KINDS.iter().find(|k| k.names.contains(&base)) {
        return Occupant::Agent(*kind);
    }
    if SHELLS.contains(&base) {
        return Occupant::Shell;
    }
    Occupant::Program(base.to_string())
}

/// One reading of the process table: process group to the name of its leader.
///
/// `-A -o` is POSIX, so this is the same command on linux and macos.
///
/// The leader is the process whose pid equals the pgid, and only that row
/// counts. Taking whichever member `ps` printed first would usually work — the
/// leader has the lowest pid in its group and `ps` prints in pid order — and
/// would quietly report a child's name whenever it did not, because pids wrap.
/// On this machine that is 19 groups in 476. Reporting a child is worse than
/// reporting nothing: `bash` under a running agent would mark the pane free for
/// another one.
///
/// A group whose leader has exited yields no entry, which the caller reads as
/// "no news" and keeps what it had.
pub fn table() -> HashMap<i32, String> {
    let mut out = HashMap::new();
    let Ok(ps) = Command::new("ps")
        .args(["-A", "-o", "pid=,pgid=,comm="])
        .output()
    else {
        return out;
    };
    for line in String::from_utf8_lossy(&ps.stdout).lines() {
        let mut parts = line.trim_start().splitn(3, char::is_whitespace);
        let (Some(pid), Some(pgid), Some(comm)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(pid), Ok(pgid)) = (pid.parse::<i32>(), pgid.trim().parse::<i32>()) else {
            continue;
        };
        if pid == pgid {
            out.insert(pgid, comm.trim().to_string());
        }
    }
    out
}

/// A finished sample, on its way back to the event loop.
///
/// `None` means the sample had nothing to say about that pane — its process
/// group ended between the pgid being read and `ps` running, which happens
/// every time a shell finishes a short command. It is not the same as "nothing
/// is running there", and overwriting a good answer with it made the state
/// glyph blink and the workspace drop out of the agents list for a frame.
#[derive(Debug)]
pub struct Reading {
    pub panes: Vec<(crate::mux::PaneId, Option<Occupant>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_agent_is_recognised_by_its_basename() {
        assert_eq!(classify("claude").agent().map(|k| k.name), Some("claude"));
        // `comm` is a full path on macos for most processes.
        assert_eq!(
            classify("/Users/x/.local/share/claude/versions/2.1.236/claude")
                .agent()
                .map(|k| k.name),
            Some("claude")
        );
        assert_eq!(classify("codex").agent().map(|k| k.name), Some("codex"));
    }

    #[test]
    fn a_shell_is_available_rather_than_an_unknown_agent() {
        for shell in ["sh", "/bin/zsh", "-zsh", "bash", "fish"] {
            let it = classify(shell);
            assert_eq!(it, Occupant::Shell, "{shell}");
            assert!(it.available(), "{shell} should be free for an agent");
            assert!(it.agent().is_none());
        }
    }

    #[test]
    fn anything_else_is_named_rather_than_guessed_at() {
        // The failure this avoids: a build reported as an agent of unknown
        // kind, which puts a glyph in the attention column for nothing.
        assert_eq!(classify("cargo"), Occupant::Program("cargo".into()));
        assert_eq!(classify("/usr/bin/vim"), Occupant::Program("vim".into()));
        assert!(!classify("cargo").available());
        assert!(classify("cargo").agent().is_none());
    }

    #[test]
    fn nothing_at_all_is_unknown() {
        assert_eq!(classify(""), Occupant::Unknown);
        assert_eq!(classify("   "), Occupant::Unknown);
    }

    #[test]
    fn the_process_table_can_be_read_and_holds_this_process() {
        let table = table();
        assert!(!table.is_empty(), "ps returned nothing");
        // This test's own process group is in there, whatever it is called.
        let mine = std::process::id() as i32;
        assert!(
            table
                .keys()
                .any(|&pgid| pgid == mine || table.contains_key(&pgid)),
            "the table has no plausible entries"
        );
    }
}
