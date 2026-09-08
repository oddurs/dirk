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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kind {
    pub name: String,
    /// Process names this agent runs under. Empty means its own name.
    pub names: Vec<String>,
    /// Fragments of a command line that identify this agent when it is run
    /// under an interpreter, and only then.
    ///
    /// An agent installed through npm is executed as `node`, so the executable
    /// says nothing and the only place its identity appears is its arguments.
    /// Consulted for nothing else: dirk does not read someone's argv to work
    /// out what they are, it reads it to disambiguate an interpreter that has
    /// told it nothing.
    pub argv: Vec<String>,
    /// How to start one, for `agent start`. Empty means dirk cannot.
    pub command: Vec<String>,
    /// Text that appears when this agent is waiting for an answer.
    ///
    /// The part of the table most likely to be wrong: these are strings another
    /// program prints, and it can change them without telling anyone. A marker
    /// that stops matching degrades to "never blocked", which is the state
    /// dirk had before any of this — bad, but not misleading.
    ///
    /// Substrings rather than patterns, deliberately. This runs against the
    /// prompt window on every tick for every agent pane, and a regular
    /// expression out of a configuration file is a way to make a redraw depend
    /// on somebody else's backtracking.
    pub blocked: Vec<String>,
    /// Whether a marker only counts alongside a menu of numbered answers.
    ///
    /// "Do you want" and "Would you like" are phrases an agent writes in
    /// ordinary prose -- "Would you like me to run the tests?" is how a finished
    /// turn ends -- with its cursor a few rows below, well inside the window.
    /// Without this, every such turn read as blocked, which masks `done` and
    /// inverts the attention order.
    pub choices: bool,
    /// Where these rules came from.
    ///
    /// Not part of what a rule *is*, and kept anyway: when a state is wrong the
    /// first question is which rules decided it, and "the ones dirk ships" and
    /// "the ones in the file you wrote last week" are very different answers.
    pub from: From,
}

/// Which of the three places a harness's rules were read from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum From {
    /// Compiled in.
    #[default]
    Shipped,
    /// An `[[agent]]` block in config.toml. What these were called first.
    Config,
    /// Its own file under `agents/`, which replaces the shipped rules whole.
    File,
}

impl From {
    pub fn name(self) -> &'static str {
        match self {
            From::Shipped => "shipped",
            From::Config => "config.toml",
            From::File => "file",
        }
    }
}

/// The harnesses dirk knows about without being told.
///
/// A catalogue, not a policy. What claude *looks like* changes every month;
/// what counts as blocked -- the rule that a marker only means blocked
/// alongside a menu -- was hard to get right and changes almost never. The
/// first belongs in a file, which is why this is only the default contents of
/// one and every field here is expressible in an `[[agent]]` block.
pub fn defaults() -> Vec<Kind> {
    let k = |name: &str, argv: &[&str], blocked: &[&str], choices: bool| Kind {
        name: name.to_string(),
        names: vec![name.to_string()],
        argv: argv.iter().map(|s| s.to_string()).collect(),
        command: vec![name.to_string()],
        blocked: blocked.iter().map(|s| s.to_string()).collect(),
        choices,
        from: From::Shipped,
    };
    vec![
        k(
            "claude",
            &["claude-code", "claude/cli.js", ".claude/local"],
            &["Do you want", "Would you like"],
            true,
        ),
        k(
            "codex",
            &["codex/cli", "openai/codex"],
            &["Allow command?", "Approve?"],
            true,
        ),
        // The four below are recognised but ship with no markers, on purpose.
        // A marker is a string another program prints, and these are ones dirk
        // has not verified. A missing one degrades to "never blocked" -- the
        // state dirk had before any of this -- while a wrong one is an
        // interruption you did not need, and bare words like "Allow" and
        // "Approve" appear in ordinary agent prose beside a numbered plan,
        // which is exactly what the menu rule looks for.
        //
        // `dirk agent hooks <name>` is the better answer for these anyway, and
        // anybody who knows the phrase can add it in four lines of config.
        k("opencode", &["opencode/cli", "opencode/bin"], &[], true),
        k(
            "aider",
            &["aider/main", "aider.main"],
            // Its marker is already the answer set, so there is no menu to find.
            &["(Y)es/(N)o", "Add to chat?"],
            false,
        ),
        k("goose", &["goose/cli"], &["Do you approve"], true),
        k("amp", &["amp/cli", "sourcegraph/amp"], &[], true),
        k("cursor-agent", &["cursor-agent"], &[], true),
        k("gemini", &["gemini/cli", "google-gemini"], &[], true),
    ]
}

/// Programs that are somebody else's identity.
///
/// A process running one of these has told us nothing about itself, so its
/// arguments are worth reading. Closed on purpose in the sense that matters:
/// this is the only case where dirk looks at a command line at all, and the
/// list says which programs are in it rather than the rule being "read
/// everybody's argv".
///
/// Interpreters were the first members. Sandboxes and container shims are the
/// same shape — `fence -- claude`, `nono run --profile claude-code -- claude`
/// — and they matter more, because an agent behind one is exactly the agent
/// most likely to be left running unattended. They are configuration rather
/// than a constant because the field adds one a month and dirk cannot ship
/// them all.
pub const INTERPRETERS: &[&str] = &["node", "python", "python3", "deno", "bun", "ruby", "perl"];

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
    pub fn agent(&self) -> Option<&Kind> {
        match self {
            Occupant::Agent(k) => Some(k),
            _ => None,
        }
    }

    /// True when an agent could be started here.
    pub fn available(&self) -> bool {
        matches!(self, Occupant::Shell)
    }
}

/// A process, as `ps` described it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Proc {
    /// The program, as invoked.
    pub program: String,
    /// Everything after it, joined. Only read when `program` is an interpreter.
    pub args: String,
}

/// Classify a process.
///
/// `wrappers` are the programs whose command line is worth reading because they
/// are running something else — interpreters, sandboxes, container shims.
pub fn identify(proc: &Proc, kinds: &[Kind], wrappers: &[String]) -> Occupant {
    let base = basename(&proc.program);
    if base.is_empty() {
        return Occupant::Unknown;
    }

    if let Some(kind) = kinds.iter().find(|k| k.names.iter().any(|n| n == base)) {
        return Occupant::Agent(kind.clone());
    }
    // Linux truncates `comm` to fifteen characters; a program invoked by path
    // is not truncated, but this is cheap insurance for the ones that are.
    if let Some(kind) = kinds
        .iter()
        .find(|k| k.names.iter().any(|n| n.starts_with(base)))
    {
        return Occupant::Agent(kind.clone());
    }

    if wrappers.iter().any(|w| w == base)
        && let Some(kind) = kinds
            .iter()
            .find(|k| k.argv.iter().any(|m| proc.args.contains(m.as_str())))
    {
        return Occupant::Agent(kind.clone());
    }

    if SHELLS.contains(&base) {
        return Occupant::Shell;
    }
    Occupant::Program(base.to_string())
}

fn basename(path: &str) -> &str {
    let base = path.rsplit('/').next().unwrap_or(path).trim();
    base.strip_prefix('-').unwrap_or(base)
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
pub fn table() -> HashMap<i32, Proc> {
    let Ok(ps) = Command::new("ps")
        .args(["-A", "-o", "pid=,pgid=,args="])
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
pub fn parse(text: &str) -> HashMap<i32, Proc> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim_start();
        // Two numbers and then the command line, which is the rest of the line
        // whatever is in it. `args` rather than `comm` because an agent run
        // under an interpreter has nothing to say in its executable name.
        let Some((pid, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Some((pgid, cmd)) = rest.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        let (Ok(pid), Ok(pgid)) = (pid.parse::<i32>(), pgid.parse::<i32>()) else {
            continue;
        };
        if pid != pgid {
            continue;
        }
        let cmd = cmd.trim();
        let (program, args) = cmd.split_once(char::is_whitespace).unwrap_or((cmd, ""));
        if !program.is_empty() {
            out.insert(
                pgid,
                Proc {
                    program: program.to_string(),
                    args: args.trim().to_string(),
                },
            );
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
    /// Launched, and has not said anything yet. Distinct from `Idle`, which
    /// looks identical and wants the opposite response: a harness that hangs on
    /// startup is not one sitting quietly.
    Starting,
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
            State::Starting => "starting",
            State::Done => "done",
            State::Working => "working",
            State::Idle => "idle",
            State::None => "unknown",
        }
    }
}

impl State {
    pub fn named(name: &str) -> Option<State> {
        Some(match name {
            "blocked" => State::Blocked,
            "starting" => State::Starting,
            "done" => State::Done,
            "working" => State::Working,
            "idle" => State::Idle,
            _ => return None,
        })
    }
}

/// What decided a state, best first.
///
/// Recorded rather than inferred because a badge you cannot explain is a badge
/// you stop believing. `agent list` reports it, so when one is wrong you can
/// see which signal was wrong.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Source {
    /// The agent said so, through a hook.
    Reported,
    /// The screen looks like it: a menu of numbered answers.
    Screen,
    /// It has produced nothing for a while, having produced something.
    Silence,
    #[default]
    None,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Reported => "reported",
            Source::Screen => "screen",
            Source::Silence => "silence",
            Source::None => "none",
        }
    }
}

/// What to install so a harness reports its own state.
///
/// The whole ladder below this is inference: argv, a window title, prose on a
/// screen, silence. Every one of them is dirk guessing at something the agent
/// already knows, and the guessing has already needed one save -- every
/// finished turn ends in a question, so a menu of numbered answers had to
/// become a requirement before a block was believed.
///
/// Guarded on `DIRK_PANE_ID`, which only exists inside a managed pane, so the
/// hook is inert everywhere else and safe to leave in a settings file for good.
pub fn hooks(kind: &str) -> String {
    // `case` rather than `[ -n "$X" ]` because this ends up inside a JSON
    // string, and a snippet somebody has to repair before pasting is a snippet
    // they will not paste. No double quotes, and no expansion outside a pane.
    let say = |state: &str| {
        format!("case $DIRK_PANE_ID in ?*) dirk agent state {state} --current;; esac")
    };
    let (done, blocked) = (say("done"), say("blocked"));
    match kind {
        "claude" => format!(
            "Add to ~/.claude/settings.json:\n\
             \n\
             {{\n\
             \x20 \"hooks\": {{\n\
             \x20   \"Stop\": [\n\
             \x20     {{ \"hooks\": [{{ \"type\": \"command\", \"command\": \"{done}\" }}] }}\n\
             \x20   ],\n\
             \x20   \"Notification\": [\n\
             \x20     {{ \"hooks\": [{{ \"type\": \"command\", \"command\": \"{blocked}\" }}] }}\n\
             \x20   ]\n\
             \x20 }}\n\
             }}\n"
        ),
        _ => format!(
            "There is no snippet for {kind} here yet, which means nobody has\n\
             written one rather than that it cannot be done.\n\
             \n\
             Anything that can run a command when a turn ends wants:\n\
             \n\
             \x20 {done}\n\
             \n\
             and when it stops to ask you something:\n\
             \n\
             \x20 {blocked}\n\
             \n\
             Both are inert outside a dirk pane, so they are safe to leave in\n\
             a settings file for good.\n"
        ),
    }
}

/// Is this agent waiting for an answer, and what makes you say that?
///
/// Only the region around the cursor is read, not the whole screen: an agent
/// that merely wrote the words "do you want" in a paragraph further up is not
/// waiting for anything.
///
/// What the screen rule found, rather than only what it concluded.
///
/// The conclusion is one bit and the interesting part is the other three: which
/// marker matched, whether a menu was required, and whether one was there. A
/// state you cannot explain is a state you stop believing, and when this one is
/// wrong it is always for one of those reasons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// The first marker present in the window, if any.
    pub marker: Option<String>,
    /// The line it appeared on, as the window has them.
    pub line: Option<String>,
    /// Whether this harness requires a menu alongside its marker.
    pub needs_menu: bool,
    /// Whether the window holds one.
    pub menu: bool,
}

impl Found {
    pub fn blocked(&self) -> bool {
        self.marker.is_some() && (!self.needs_menu || self.menu)
    }

    /// Why this is not `blocked`, in the words somebody debugging it needs.
    pub fn why_not(&self) -> Option<&'static str> {
        match (self.marker.is_some(), self.needs_menu && !self.menu) {
            (false, _) => Some("no marker for this harness is on the screen"),
            (true, true) => Some("a marker matched, but there is no menu of numbered answers"),
            _ => None,
        }
    }
}

/// Run the screen rule and keep its working.
///
/// The one place the rule lives, so that what `agent explain` reports and what
/// the nav believes cannot be two different rules that happen to agree today.
pub fn examine(kind: &Kind, window: &str) -> Found {
    let marker = kind
        .blocked
        .iter()
        .find(|m| window.contains(m.as_str()))
        .cloned();
    let line = marker.as_ref().and_then(|m| {
        window
            .lines()
            .find(|l| l.contains(m.as_str()))
            .map(|l| l.trim().to_string())
    });
    Found {
        marker,
        line,
        needs_menu: kind.choices,
        menu: has_menu(window),
    }
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
    /// Panes whose harness was found behind a wrapper, and which wrapper.
    ///
    /// Kept so `agent explain` can say that, rather than leaving somebody to
    /// wonder how dirk came to recognise a program called `fence`.
    pub hinted: Vec<(crate::mux::PaneId, String)>,
}

/// Was this agent found behind something, rather than being it?
///
/// True when the foreground program is not one of the harness's own names,
/// which is the only way the argv rule can have been what matched.
pub fn behind(proc: &Proc, kind: &Kind) -> Option<String> {
    let base = basename(&proc.program);
    let its_own = kind.names.iter().any(|n| n == base || n.starts_with(base));
    (!its_own).then(|| base.to_string())
}

#[cfg(test)]
mod tests {
    fn blocked(kind: &super::Kind, window: &str) -> bool {
        super::examine(kind, window).blocked()
    }

    use super::*;

    /// Most of these ask about a program name alone, which is the common case.
    fn classify(program: &str) -> Occupant {
        with_args(program, "")
    }

    fn with_args(program: &str, args: &str) -> Occupant {
        let wrappers: Vec<String> = INTERPRETERS.iter().map(|s| s.to_string()).collect();
        identify(
            &Proc {
                program: program.to_string(),
                args: args.to_string(),
            },
            &defaults(),
            &wrappers,
        )
    }

    #[test]
    fn a_known_agent_is_recognised_by_its_basename() {
        assert_eq!(
            classify("claude").agent().map(|k| k.name.as_str()),
            Some("claude")
        );
        // `comm` is a full path on macos for most processes.
        assert_eq!(
            classify("/Users/x/.local/share/claude/versions/2.1.236/claude")
                .agent()
                .map(|k| k.name.as_str()),
            Some("claude")
        );
        assert_eq!(
            classify("codex").agent().map(|k| k.name.as_str()),
            Some("codex")
        );
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

    /// Real `ps -A -o pid=,pgid=,args=` output, padding and all.
    const PS: &str = "\
    1       1 /sbin/launchd
  337     337 /usr/libexec/logd
18776   18776 claude
61207   84361 sleep 30
84361   84361 /bin/zsh -l
99123   99123 node /Users/x/.npm/lib/node_modules/@anthropic-ai/claude-code/cli.js
  512     512 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --type=renderer
";

    #[test]
    fn the_padding_between_columns_is_not_a_field() {
        // What went wrong: `ps` right-aligns its numbers, so splitting on each
        // whitespace character puts an empty string where the pgid should be
        // and every line is discarded. The table came back empty on both
        // platforms and said nothing, because an empty table reads as "no news".
        let table = parse(PS);
        assert!(!table.is_empty(), "the whole table was discarded");
        assert_eq!(
            table.get(&1).map(|p| p.program.as_str()),
            Some("/sbin/launchd")
        );
        assert_eq!(
            table.get(&18776).map(|p| p.program.as_str()),
            Some("claude")
        );
    }

    #[test]
    fn only_the_group_leader_names_its_group() {
        let table = parse(PS);
        // Group 84361 is led by pid 84361 (`zsh`) but `ps` lists pid 61207
        // (`sleep`) first, because pids wrap. Taking the first row would report
        // a child -- and a `bash` under a running agent would mark the pane
        // free for another one.
        assert_eq!(
            table.get(&84361).map(|p| p.program.as_str()),
            Some("/bin/zsh")
        );
    }

    #[test]
    fn a_group_whose_leader_has_gone_has_no_entry() {
        // Better than naming a survivor: the caller reads a miss as "no news"
        // and keeps the last good answer.
        let table = parse("61207   84361 sleep\n");
        assert!(table.is_empty());
    }

    #[test]
    fn an_approval_prompt_reads_as_blocked() {
        let kinds = defaults();
        let find = |n: &str| kinds.iter().find(|k| k.name == n).unwrap();
        let claude = find("claude");
        assert!(blocked(
            claude,
            "Do you want to proceed?\n  1. Yes\n  2. No"
        ));
        assert!(!blocked(claude, "Reading src/agent.rs\nWriting tests"));
        // A marker belongs to one agent, not to all of them.
        assert!(!blocked(find("codex"), "Do you want to proceed?"));
    }

    #[test]
    fn every_kind_can_be_found_and_most_can_be_read() {
        // Everything shipped has to be recognisable, or it is an entry that
        // does nothing.
        for kind in &defaults() {
            assert!(!kind.names.is_empty(), "{} matches nothing", kind.name);
            assert!(!kind.command.is_empty(), "{} cannot be started", kind.name);
        }
        // Markers are a different promise. A kind without them relies on the
        // hook and is never falsely blocked; a kind with them must not carry
        // one that matches ordinary prose.
        //
        // What separates a prompt from a word is a second word or some
        // punctuation. "Approve?" is a question being asked and "(Y)es/(N)o" is
        // an answer set; "Approve" is half of "Approved by", and the menu rule
        // does not save you from it -- an agent printing a numbered plan
        // produces exactly the menu that rule looks for.
        for kind in &defaults() {
            for marker in &kind.blocked {
                let asks = marker.contains(' ') || marker.chars().any(|c| !c.is_alphanumeric());
                assert!(asks, "{}: {marker:?} is a word, not a prompt", kind.name);
            }
        }
        // And the ones dirk was built against still carry theirs.
        for name in ["claude", "codex", "aider"] {
            let kind = defaults().into_iter().find(|k| k.name == name).unwrap();
            assert!(!kind.blocked.is_empty(), "{name} lost its markers");
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
            let marks = crate::glyph::Glyphs::default();
            assert_ne!(
                marks.text(crate::glyph::G::state(word)),
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
        assert!(table.values().all(|p| !p.program.is_empty()));
    }
}
