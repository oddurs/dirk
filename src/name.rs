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

//! Naming workspaces from the agent's own intent — namesync, in process.
//!
//! This is a port of the `namesync` plugin's `policy.js` and `naming.js`. The
//! policy is unchanged; what changed is everything around it. As a herdr plugin
//! it had to subscribe to a socket, diff titles it was handed, and write names
//! back through an API. Inside dirk the title arrives from the pane's own OSC
//! callback and the label is a `String` two structs away, so the plugin's
//! daemon, client, state store and sinks all disappear. What is left is the
//! part that was ever interesting: deciding *when* a name should change.
//!
//! The one rule worth restating, because it is what stops the thing fighting
//! you: **if the label is not the one dirk last wrote, a human wrote it**, and
//! it is never touched again.
//!
//! No regex crate. Every pattern here is a handful of character tests, and a
//! regex engine to run them would be more dependency than policy.

use crate::config::Naming;
use crate::mux::session::NameState;
use std::time::Instant;

/// Words that survive in a title but say nothing about what the work *is*.
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "for", "to", "of", "in", "on", "at", "by", "with",
    "from", "into", "via", "is", "are", "was", "be", "this", "that", "it", "its", "my", "our",
    "your", "some", "new", "up", "out",
];

/// Commands a pane shows as its title before an agent takes over.
const COMMAND_VERBS: &[&str] = &[
    "cd", "ls", "ll", "cat", "tail", "head", "less", "man", "git", "gh", "npm", "npx", "pnpm",
    "yarn", "bun", "node", "deno", "python", "python3", "pip", "cargo", "rustc", "go", "make",
    "just", "docker", "kubectl", "ssh", "scp", "sudo", "brew", "apt", "vim", "nvim", "emacs", "hx",
    "code", "claude", "codex", "pi", "tmux", "herdr", "dirk", "clear", "exit",
];

/// herdr's placeholder shapes, kept because dirk generates the same kinds.
const PLACEHOLDER_NOUNS: &[&str] = &["tab", "window", "pane", "workspace", "shell", "terminal"];

/// Strip the spinner braille and state glyphs agents prefix to their titles,
/// then collapse whitespace.
pub fn normalize(title: &str) -> String {
    let trimmed = title.trim_start_matches(|c: char| {
        c.is_whitespace()
            || c == '*'
            || ('\u{2800}'..='\u{28FF}').contains(&c)   // braille spinners
            || ('\u{2500}'..='\u{27BF}').contains(&c) // box drawing, dingbats
    });
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Crude stemming only: enough that "plugin"/"plugins" and "rename"/"renaming"
/// compare equal, so a reworded title is not mistaken for a new intent.
fn stem(word: &str) -> String {
    let mut w = word.to_string();
    if w.len() > 4 && w.ends_with("ing") {
        w.truncate(w.len() - 3);
    } else if w.len() > 4 && w.ends_with("ed") {
        w.truncate(w.len() - 2);
    }
    if w.len() > 3 && w.ends_with("es") {
        w.truncate(w.len() - 2);
    } else if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
        w.truncate(w.len() - 1);
    }
    // Collapse the silent -e so "rename" and "renaming" land on one stem.
    if w.len() > 4 && w.ends_with('e') {
        w.truncate(w.len() - 1);
    }
    w
}

fn tokens(text: &str) -> Vec<String> {
    normalize(text)
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() > 1 && !STOP_WORDS.contains(t))
        .map(stem)
        .collect()
}

/// Jaccard overlap of significant tokens. 1 = same intent, 0 = unrelated.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (mut x, mut y) = (tokens(a), tokens(b));
    x.sort_unstable();
    x.dedup();
    y.sort_unstable();
    y.dedup();
    if x.is_empty() && y.is_empty() {
        return 1.0;
    }
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    let shared = x.iter().filter(|t| y.contains(t)).count();
    shared as f32 / (x.len() + y.len() - shared) as f32
}

/// True when a title says nothing about intent and must never become a name.
pub fn is_junk(title: &str, repo: &str, ignore: &[String]) -> bool {
    let t = normalize(title);
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();

    // Programs that show their own name as a title. Without this they arrive
    // as intents: a pane running `nvim` would be a workspace called "nvim".
    if ignore.iter().any(|x| x.eq_ignore_ascii_case(&lower)) {
        return true;
    }

    // A shell prompt: "oddurs@Oddurs-MacBook-Pro:~/Code/perfect".
    if let Some(colon) = t.find(':') {
        let head = &t[..colon];
        if !head.contains(char::is_whitespace) && head.contains('@') {
            return true;
        }
        // A Windows drive letter: "c:\src".
        if colon == 1 && t[colon + 1..].starts_with(['\\', '/']) {
            return true;
        }
    }

    // A bare path or filename is location, not intent.
    if t.starts_with(['~', '/', '.']) {
        return true;
    }

    // A command line the shell echoed before the agent started. The verb alone
    // is not enough — "go", "make" and "less" are ordinary words, and "go
    // routine leak fix" is a perfectly good intent — so the title must also
    // *look* like an invocation: short, or carrying a flag, path or path-ish
    // character.
    let words: Vec<&str> = t.split_whitespace().collect();
    if let Some(first) = words.first() {
        let verb = first.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
        if COMMAND_VERBS.contains(&verb.to_lowercase().as_str())
            && (words.len() <= 2 || has_flag(&t) || t.contains(['/', '~', '=']))
        {
            return true;
        }
    }
    if has_long_flag(&t) {
        return true;
    }

    // The repo name alone is what dirk already defaults to.
    if !repo.is_empty() && lower == repo.to_lowercase() {
        return true;
    }

    // Needs at least one real word.
    tokens(&t).is_empty()
}

/// ` -x` or ` --x`
fn has_flag(t: &str) -> bool {
    t.split_whitespace().skip(1).any(|w| {
        let w = w
            .strip_prefix("--")
            .or_else(|| w.strip_prefix('-'))
            .unwrap_or("");
        w.starts_with(|c: char| c.is_ascii_alphabetic())
    })
}

/// ` --x` only
fn has_long_flag(t: &str) -> bool {
    t.split_whitespace().skip(1).any(|w| {
        w.strip_prefix("--")
            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_lowercase()))
    })
}

/// Distinguishes a label a human chose from the one dirk generated. Without
/// this, every fresh workspace would look hand-named and never be claimed.
pub fn looks_auto_generated(name: &str, repo: &str, branch: &str) -> bool {
    let t = normalize(name);
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();

    if (!repo.is_empty() && lower == repo.to_lowercase())
        || (!branch.is_empty() && lower == branch.to_lowercase())
    {
        return true;
    }

    // "w3", "w3:t1"
    if let Some(rest) = lower.strip_prefix('w') {
        let (n, t2) = match rest.split_once(":t") {
            Some((a, b)) => (a, Some(b)),
            None => (rest, None),
        };
        if !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
            && t2.is_none_or(|b| !b.is_empty() && b.chars().all(|c| c.is_ascii_digit()))
        {
            return true;
        }
    }

    // "tab", "tab 2", "shell", "pane 3"
    let mut parts = lower.split_whitespace();
    if let Some(noun) = parts.next()
        && PLACEHOLDER_NOUNS.contains(&noun)
    {
        match parts.next() {
            None => return true,
            Some(n) if n.chars().all(|c| c.is_ascii_digit()) && parts.next().is_none() => {
                return true;
            }
            _ => {}
        }
    }

    // A bare number.
    lower.chars().all(|c| c.is_ascii_digit())
}

/// Drop a leading project name from an intent, so a row already sitting under
/// "ptop" does not read "ptop-adopt-remaining-lessons" underneath it. Never
/// strips everything: "cairn" under "cairn" stays as it is.
pub fn strip_project(text: &str, project: &str) -> String {
    let t = normalize(text);
    if t.is_empty() || project.is_empty() {
        return t;
    }
    if !t.to_lowercase().starts_with(&project.to_lowercase()) {
        return t;
    }
    let rest = t[project.len()..].trim_start_matches([' ', ':', '_', '/', '-']);
    if rest.is_empty() || tokens(rest).is_empty() {
        return t;
    }
    let mut c = rest.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => t,
    }
}

/// The longest an agent name may be, matching herdr's `[a-z][a-z0-9_-]{0,31}`.
pub const AGENT_NAME_MAX: usize = 32;

/// Turn an intent into a name you can type.
///
/// Ported from namesync, where it made agent names out of the same intents that
/// make workspace labels. A name has to be typed at a command line, so it is
/// lowercase, hyphenated, starts with a letter and has a length.
pub fn slugify(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in normalize(text).to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let s = out.trim_matches('-');
    if s.is_empty() {
        return String::new();
    }
    // Must start with a letter, so a name is never mistaken for a number.
    let mut s = if s.starts_with(|c: char| c.is_ascii_alphabetic()) {
        s.to_string()
    } else {
        format!("x-{s}")
    };

    if s.chars().count() > max {
        s = s.chars().take(max).collect();
        // Cut at a word boundary when one is close, rather than mid-word.
        if let Some(cut) = s.rfind('-')
            && cut >= max * 6 / 10
        {
            s.truncate(cut);
        }
    }
    s.trim_end_matches('-').to_string()
}

/// A name no live agent already has.
///
/// Names must be unique among live agents, because they are how one is
/// addressed. A collision takes a suffix, and the base is shortened to make
/// room rather than the name growing past its limit.
pub fn unique(desired: &str, taken: &std::collections::HashSet<String>) -> String {
    if desired.is_empty() {
        return String::new();
    }
    if !taken.contains(desired) {
        return desired.to_string();
    }
    for i in 2..100u32 {
        let suffix = format!("-{i}");
        let room = AGENT_NAME_MAX.saturating_sub(suffix.len());
        let base: String = desired.chars().take(room).collect();
        let candidate = format!("{}{suffix}", base.trim_end_matches('-'));
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    String::new()
}

/// Why a rename did not happen. Carried so the status line and the log can
/// explain themselves instead of silently doing nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Disabled,
    /// The title is the question it is asking, not the work it is doing.
    Blocked,
    NoIntent,
    Junk,
    Manual,
    Unchanged,
    Similar,
    RateLimited,
    Settling,
    MultiPane,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Rename(String),
    Skip(Skip),
}

/// Decide whether `title` should become the workspace's label.
///
/// Pure apart from the clock, so the test suite drives it directly.
#[allow(clippy::too_many_arguments)]
pub fn decide(
    cfg: &Naming,
    state: &mut NameState,
    current: &str,
    title: Option<&str>,
    repo: &str,
    branch: &str,
    pane_count: usize,
    blocked: bool,
    now: Instant,
) -> Decision {
    if !cfg.enabled {
        return Decision::Skip(Skip::Disabled);
    }
    // A blocked agent's title describes the dialog, not the work. Naming from
    // it produces a label like "Do you want to proceed", which is wrong and
    // sticky -- it is what the workspace is called until the next intent.
    //
    // This branch was written when the policy was ported and could not be
    // reached: dirk had no idea what blocked meant until 0031.
    if blocked && cfg.skip_while_blocked {
        // Forgotten, not merely skipped. `rename_pass` runs on every chunk of
        // output and the state is recomputed once per frame, so the pass that
        // first sees the question still sees `working` and records it as
        // pending. Skipping every later pass then leaves it there, and the
        // moment the block clears it is committed -- which is exactly the label
        // this guard exists to prevent.
        state.pending = None;
        return Decision::Skip(Skip::Blocked);
    }
    // A workspace holding two panes has no single intent, so neither title
    // speaks for it.
    if pane_count > 1 {
        return Decision::Skip(Skip::MultiPane);
    }
    let Some(raw) = title else {
        return Decision::Skip(Skip::NoIntent);
    };
    let intent = normalize(raw);
    if intent.is_empty() {
        return Decision::Skip(Skip::NoIntent);
    }
    if is_junk(&intent, repo, &cfg.ignore_titles) {
        return Decision::Skip(Skip::Junk);
    }

    // The heart of "rename only when it makes sense": if the live label is not
    // the one dirk last wrote, a human changed it. Back off permanently. A
    // label dirk generated itself is nobody's choice, so it stays adoptable.
    let cur = normalize(current);
    if cfg.respect_manual_names && !cur.is_empty() && !looks_auto_generated(&cur, repo, branch) {
        let ours = state.applied.as_deref().map(normalize);
        if ours.as_deref() != Some(cur.as_str()) {
            return Decision::Skip(Skip::Manual);
        }
    }

    let desired = if cfg.strip_project_prefix {
        strip_project(&intent, repo)
    } else {
        intent.clone()
    };
    if desired.is_empty() {
        return Decision::Skip(Skip::NoIntent);
    }
    if cur == normalize(&desired) {
        state.pending = None;
        return Decision::Skip(Skip::Unchanged);
    }

    // Titles churn early in a turn. Wait for the intent to hold still rather
    // than chasing every revision the agent publishes.
    match &state.pending {
        Some((t, since)) if *t == desired => {
            if now.duration_since(*since).as_millis() < cfg.debounce_ms as u128 {
                return Decision::Skip(Skip::Settling);
            }
        }
        _ => {
            state.pending = Some((desired.clone(), now));
            return Decision::Skip(Skip::Settling);
        }
    }

    if let Some(last) = state.last_rename
        && now.duration_since(last).as_millis() < cfg.min_interval_ms as u128
    {
        return Decision::Skip(Skip::RateLimited);
    }

    // A reworded title describing the same work is not a new intent.
    if !cur.is_empty() && similarity(&cur, &desired) >= cfg.similarity_threshold {
        state.pending = None;
        return Decision::Skip(Skip::Similar);
    }

    state.pending = None;
    state.last_rename = Some(now);
    state.applied = Some(desired.clone());
    Decision::Rename(desired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn cfg() -> Naming {
        Naming {
            debounce_ms: 0,
            min_interval_ms: 0,
            ..Naming::default()
        }
    }

    fn decide_once(state: &mut NameState, current: &str, title: &str, repo: &str) -> Decision {
        let now = Instant::now();
        // Debounce needs two passes: the first records the title, the second
        // finds it unchanged and commits.
        decide(&cfg(), state, current, Some(title), repo, "", 1, false, now);
        decide(&cfg(), state, current, Some(title), repo, "", 1, false, now)
    }

    #[test]
    fn adopts_an_intent_over_a_default_label() {
        let mut st = NameState::default();
        assert_eq!(
            decide_once(
                &mut st,
                "bedreader",
                "Open source wifi e-reader",
                "bedreader"
            ),
            Decision::Rename("Open source wifi e-reader".into())
        );
    }

    #[test]
    fn a_hand_written_name_is_never_touched() {
        let mut st = NameState::default();
        // dirk never wrote this label, and it is not a default shape.
        assert_eq!(
            decide_once(
                &mut st,
                "my careful name",
                "Open source wifi e-reader",
                "bedreader"
            ),
            Decision::Skip(Skip::Manual)
        );
    }

    #[test]
    fn labels_dirk_wrote_stay_adoptable() {
        let mut st = NameState {
            applied: Some("Old intent".into()),
            ..Default::default()
        };
        assert!(matches!(
            decide_once(&mut st, "Old intent", "Totally different work now", "repo"),
            Decision::Rename(_)
        ));
    }

    #[test]
    fn placeholders_are_adoptable() {
        for label in ["w3", "tab 2", "shell", "7", "bedreader"] {
            let mut st = NameState::default();
            assert!(
                matches!(
                    decide_once(&mut st, label, "Open source wifi e-reader", "bedreader"),
                    Decision::Rename(_)
                ),
                "{label} should have been claimed"
            );
        }
    }

    #[test]
    fn a_rewording_is_not_a_new_intent() {
        // The plugin's own example: "naming plugin" -> "naming plugins".
        assert!(
            similarity(
                "Herdr session naming plugin",
                "Herdr session naming plugins"
            ) >= 0.6
        );
        // The label has to be one dirk wrote, or the manual-name guard fires
        // first — which is the correct order, and the order policy.js uses.
        let mut st = NameState {
            applied: Some("Herdr session naming plugin".into()),
            ..Default::default()
        };
        assert_eq!(
            decide_once(
                &mut st,
                "Herdr session naming plugin",
                "Herdr session naming plugins",
                "x"
            ),
            Decision::Skip(Skip::Similar)
        );
    }

    #[test]
    fn junk_never_becomes_a_name() {
        let junk = [
            "oddurs@Oddurs-MacBook-Pro:~/Code/perfect", // a prompt
            "~/Code/dirk",                              // a path
            "./run.sh",
            "cd namesync", // an echoed command
            "claude --dangerously-skip-permissions",
            "git",
            "bedreader", // the repo name is what dirk already defaults to
        ];
        for t in junk {
            assert!(is_junk(t, "bedreader", &[]), "{t} should be junk");
        }
    }

    #[test]
    fn a_program_that_titles_itself_is_not_an_intent() {
        // Without the list these become workspace names: a pane running nvim
        // would be a workspace called "nvim".
        let ignore = crate::config::Naming::default().ignore_titles;
        for t in ["nvim", "lazygit", "Claude Code", "htop"] {
            assert!(is_junk(t, "dirk", &ignore), "{t} should be ignored");
            // And without the list, most of them get through.
        }
        assert!(
            !is_junk("lazygit", "dirk", &[]),
            "the list is what rejects it"
        );
    }

    #[test]
    fn ordinary_words_that_happen_to_be_commands_are_kept() {
        // The policy's own caveat: "go", "make" and "less" are English too.
        for t in ["go routine leak fix", "make the sidebar clickable"] {
            assert!(!is_junk(t, "dirk", &[]), "{t} should have survived");
        }
    }

    #[test]
    fn the_project_name_is_stripped_from_the_intent() {
        // A row already sitting under "ptop" should not read "ptop-adopt-...".
        assert_eq!(
            strip_project("ptop-adopt-remaining-lessons", "ptop"),
            "Adopt-remaining-lessons"
        );
        // But never strip everything away.
        assert_eq!(strip_project("cairn", "cairn"), "cairn");
    }

    #[test]
    fn a_title_must_hold_still_before_it_is_committed() {
        let cfg = Naming {
            debounce_ms: 1000,
            min_interval_ms: 0,
            ..Naming::default()
        };
        let mut st = NameState::default();
        let t0 = Instant::now();

        // First sighting only starts the clock.
        assert_eq!(
            decide(
                &cfg,
                &mut st,
                "w1",
                Some("Building the mux core"),
                "dirk",
                "",
                1,
                false,
                t0
            ),
            Decision::Skip(Skip::Settling)
        );
        // Still churning.
        assert_eq!(
            decide(
                &cfg,
                &mut st,
                "w1",
                Some("Building the mux core"),
                "dirk",
                "",
                1,
                false,
                t0
            ),
            Decision::Skip(Skip::Settling)
        );
        // Settled.
        let later = t0 + Duration::from_millis(1500);
        assert!(matches!(
            decide(
                &cfg,
                &mut st,
                "w1",
                Some("Building the mux core"),
                "dirk",
                "",
                1,
                false,
                later
            ),
            Decision::Rename(_)
        ));
    }

    #[test]
    fn one_rename_per_workspace_per_interval() {
        let cfg = Naming {
            debounce_ms: 0,
            min_interval_ms: 30_000,
            ..Naming::default()
        };
        let mut st = NameState::default();
        let t0 = Instant::now();
        decide(
            &cfg,
            &mut st,
            "w1",
            Some("First intent"),
            "dirk",
            "",
            1,
            false,
            t0,
        );
        assert!(matches!(
            decide(
                &cfg,
                &mut st,
                "w1",
                Some("First intent"),
                "dirk",
                "",
                1,
                false,
                t0
            ),
            Decision::Rename(_)
        ));

        // A second, unrelated intent arriving immediately is rate limited.
        decide(
            &cfg,
            &mut st,
            "First intent",
            Some("Wholly unrelated topic"),
            "dirk",
            "",
            1,
            false,
            t0,
        );
        assert_eq!(
            decide(
                &cfg,
                &mut st,
                "First intent",
                Some("Wholly unrelated topic"),
                "dirk",
                "",
                1,
                false,
                t0
            ),
            Decision::Skip(Skip::RateLimited)
        );
    }

    #[test]
    fn a_workspace_holding_two_panes_has_no_single_intent() {
        let mut st = NameState::default();
        assert_eq!(
            decide(
                &cfg(),
                &mut st,
                "w1",
                Some("Some work"),
                "dirk",
                "",
                2,
                false,
                Instant::now()
            ),
            Decision::Skip(Skip::MultiPane)
        );
    }

    #[test]
    fn a_blocked_agent_is_not_named_after_the_question_it_is_asking() {
        let mut st = NameState::default();
        let now = Instant::now();
        assert_eq!(
            decide(
                &cfg(),
                &mut st,
                "w1",
                Some("Do you want to proceed?"),
                "dirk",
                "",
                1,
                true,
                now
            ),
            Decision::Skip(Skip::Blocked)
        );
        // And the name it had is untouched by the block.
        assert!(st.applied.is_none());
    }

    #[test]
    fn a_name_is_something_you_could_type() {
        assert_eq!(
            slugify("Building the mux core", 32),
            "building-the-mux-core"
        );
        assert_eq!(
            slugify("feat/packaging-manifests", 32),
            "feat-packaging-manifests"
        );
        // Starts with a letter, whatever the intent started with.
        assert_eq!(slugify("0031 real states", 32), "x-0031-real-states");
        assert_eq!(slugify("!!!", 32), "");
    }

    #[test]
    fn a_long_name_is_cut_at_a_word() {
        let n = slugify("Reading the vt100 grid and writing it into ratatui", 24);
        assert!(n.chars().count() <= 24, "{n}");
        assert!(!n.ends_with('-'));
        // Cut at a boundary rather than mid-word, when one is close enough.
        assert_eq!(n, "reading-the-vt100-grid");
    }

    #[test]
    fn two_agents_doing_the_same_thing_get_different_names() {
        let mut taken = std::collections::HashSet::new();
        let first = unique("reviewer", &taken);
        assert_eq!(first, "reviewer");
        taken.insert(first);
        assert_eq!(unique("reviewer", &taken), "reviewer-2");
    }

    #[test]
    fn a_suffix_never_pushes_a_name_past_its_limit() {
        let long = "a".repeat(AGENT_NAME_MAX);
        let mut taken = std::collections::HashSet::new();
        taken.insert(long.clone());
        let next = unique(&long, &taken);
        assert!(next.chars().count() <= AGENT_NAME_MAX, "{next}");
        assert!(next.ends_with("-2"));
    }

    #[test]
    fn spinner_braille_is_stripped_before_anything_else() {
        assert_eq!(
            normalize("⠹ Building the mux core"),
            "Building the mux core"
        );
        assert_eq!(
            normalize("  ✳ Thinking about naming  "),
            "Thinking about naming"
        );
    }
}
