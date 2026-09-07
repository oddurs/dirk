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
    /// Whether a marker only counts alongside a menu of numbered answers.
    ///
    /// "Do you want" and "Would you like" are phrases an agent writes in
    /// ordinary prose -- "Would you like me to run the tests?" is how a finished
    /// turn ends -- with its cursor a few rows below, well inside the window.
    /// Without this, every such turn read as blocked, which masks `done` and
    /// inverts the attention order.
    choices: bool,
    /// Text that appears when this agent is waiting for an answer.
    ///
    /// The part of this file most likely to be wrong: these are strings another
    /// program prints, and it can change them without telling anyone. A marker
    /// that stops matching degrades to "never blocked", which is the state
    /// dirk had before any of this — bad, but not misleading.
    blocked: &'static [&'static str],
}

/// The table. Adding an agent is an entry.
pub const KINDS: &[Kind] = &[
    Kind {
        name: "claude",
        names: &["claude"],
        blocked: &["Do you want", "Would you like"],
        choices: true,
    },
    Kind {
        name: "codex",
        names: &["codex"],
        blocked: &["Allow command?", "Approve?"],
        choices: true,
    },
    Kind {
        name: "aider",
        names: &["aider"],
        blocked: &["(Y)es/(N)o", "Add to chat?"],
        // Its marker is already the answer set, so there is no menu to find.
        choices: false,
    },
    Kind {
        name: "goose",
        names: &["goose"],
        blocked: &["Do you approve"],
        choices: true,
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
    let Ok(ps) = Command::new("ps")
        .args(["-A", "-o", "pid=,pgid=,comm="])
        .output()
    else {
        return HashMap::new();
    };
    parse(&String::from_utf8_lossy(&ps.stdout))
}

/// The parsing half, separated so it can be tested against real output instead
/// of against whatever the machine running the tests happens to have.
///
/// It was not, once, and the bug that hid there was this: `ps` right-aligns its
/// numeric columns, so the gap between pid and pgid is several spaces, and
/// splitting on each whitespace character yields an empty field where the pgid
/// should be. Every line was discarded and the table came back empty — on both
/// platforms, silently, since an empty table just means "no news about any
/// pane".
pub fn parse(text: &str) -> HashMap<i32, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(pgid)) = (fields.next(), fields.next()) else {
            continue;
        };
        let (Ok(pid), Ok(pgid)) = (pid.parse::<i32>(), pgid.parse::<i32>()) else {
            continue;
        };
        if pid != pgid {
            continue;
        }
        // A command can contain spaces ("Google Chrome Helper"), so it is the
        // whole of the rest of the line rather than the next field.
        let comm = fields.collect::<Vec<_>>().join(" ");
        if !comm.is_empty() {
            out.insert(pgid, comm);
        }
    }
    out
}

/// What an agent is doing.
///
/// `Done` and `Idle` are the same underlying state — the agent is waiting for
/// you — and what separates them is whether you have looked since it stopped.
/// That distinction is the whole reason `Done` exists: finished work nobody has
/// noticed is the thing worth showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// Waiting on a human. The only state that is owed something.
    Blocked,
    /// Finished, and not looked at since.
    Done,
    /// Producing output.
    Working,
    /// Waiting, and seen.
    Idle,
    /// No agent here at all.
    #[default]
    None,
}

impl State {
    /// The word `THEME::agent_state` and the nav both speak.
    pub fn glyph_name(self) -> &'static str {
        match self {
            State::Blocked => "blocked",
            State::Done => "done",
            State::Working => "working",
            State::Idle => "idle",
            State::None => "unknown",
        }
    }
}

/// Is this agent waiting for an answer?
///
/// Only the region around the cursor is read, not the whole screen: an agent
/// that merely wrote the words "do you want" in a paragraph further up is not
/// waiting for anything.
pub fn is_blocked(kind: Kind, window: &str) -> bool {
    if !kind.blocked.iter().any(|m| window.contains(m)) {
        return false;
    }
    !kind.choices || has_menu(window)
}

/// Two or more numbered answers, which is what an approval box is.
///
/// One is not a menu: an agent writing "1. First we should..." in prose is
/// listing, not asking.
fn has_menu(window: &str) -> bool {
    window
        .lines()
        .filter(|line| {
            let t = line
                .trim_start()
                .trim_start_matches(['❯', '>', '*', '·', ' ']);
            let mut c = t.chars();
            matches!((c.next(), c.next()), (Some(d), Some('.')) if d.is_ascii_digit())
        })
        .count()
        >= 2
}

/// How many rows above the cursor count as "the prompt".
///
/// Anchored to the cursor rather than to the bottom of the screen. A waiting
/// agent has its cursor in or just under the question it is asking, wherever
/// that has ended up — which on a screen that is not yet full is nowhere near
/// the bottom.
pub const PROMPT_ROWS: u16 = 12;

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

    /// Real `ps -A -o pid=,pgid=,comm=` output, padding and all.
    const PS: &str = "\
    1       1 /sbin/launchd
  337     337 /usr/libexec/logd
18776   18776 claude
61207   84361 sleep
84361   84361 /bin/zsh
99123   84361 node
  512     512 Google Chrome Helper
";

    #[test]
    fn the_padding_between_columns_is_not_a_field() {
        // What went wrong: `ps` right-aligns its numbers, so splitting on each
        // whitespace character puts an empty string where the pgid should be
        // and every line is discarded. The table came back empty on both
        // platforms and said nothing, because an empty table reads as "no news".
        let table = parse(PS);
        assert!(!table.is_empty(), "the whole table was discarded");
        assert_eq!(table.get(&1).map(String::as_str), Some("/sbin/launchd"));
        assert_eq!(table.get(&18776).map(String::as_str), Some("claude"));
    }

    #[test]
    fn only_the_group_leader_names_its_group() {
        let table = parse(PS);
        // Group 84361 is led by pid 84361 (`zsh`) but `ps` lists pid 61207
        // (`sleep`) first, because pids wrap. Taking the first row would report
        // a child -- and a `bash` under a running agent would mark the pane
        // free for another one.
        assert_eq!(table.get(&84361).map(String::as_str), Some("/bin/zsh"));
    }

    #[test]
    fn a_group_whose_leader_has_gone_has_no_entry() {
        // Better than naming a survivor: the caller reads a miss as "no news"
        // and keeps the last good answer.
        let table = parse("61207   84361 sleep\n");
        assert!(table.is_empty());
    }

    #[test]
    fn a_command_with_spaces_survives_intact() {
        assert_eq!(
            parse(PS).get(&512).map(String::as_str),
            Some("Google Chrome Helper")
        );
    }

    #[test]
    fn an_approval_prompt_reads_as_blocked() {
        let claude = KINDS.iter().find(|k| k.name == "claude").unwrap();
        assert!(is_blocked(
            *claude,
            "Do you want to proceed?\n  1. Yes\n  2. No"
        ));
        assert!(!is_blocked(*claude, "Reading src/agent.rs\nWriting tests"));
        // A marker belongs to one agent, not to all of them.
        let codex = KINDS.iter().find(|k| k.name == "codex").unwrap();
        assert!(!is_blocked(*codex, "Do you want to proceed?"));
    }

    #[test]
    fn every_kind_says_how_to_tell_it_is_waiting() {
        // A kind with no markers can never be blocked, which is the state dirk
        // had before any of this and not worth shipping again by omission.
        for kind in KINDS {
            assert!(
                !kind.blocked.is_empty(),
                "{} has no blocked markers",
                kind.name
            );
            assert!(!kind.names.is_empty(), "{} matches nothing", kind.name);
        }
    }

    #[test]
    fn the_state_words_are_the_ones_the_theme_knows() {
        // `THEME::agent_state` matches on these strings; a mismatch is a silent
        // fallback to a blank glyph.
        for (state, word) in [
            (State::Blocked, "blocked"),
            (State::Done, "done"),
            (State::Working, "working"),
            (State::Idle, "idle"),
        ] {
            assert_eq!(state.glyph_name(), word);
            assert_ne!(
                crate::theme::THEME.agent_state(word).0,
                " ",
                "{word} draws nothing"
            );
        }
    }

    #[test]
    fn the_real_process_table_is_readable() {
        // The fixture tests cover the parsing; this one only proves `ps` is
        // where it is expected to be and speaks the expected dialect.
        let table = table();
        assert!(
            !table.is_empty(),
            "ps returned nothing usable on this machine"
        );
        assert!(table.values().all(|c| !c.is_empty()));
    }
}
