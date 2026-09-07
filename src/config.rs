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

//! Configuration: the few things that are yours rather than dirk's.
//!
//! Everything here has a working default, so dirk runs with no config file at
//! all. `~/.config/dirk/config.toml` overrides what it names and leaves the
//! rest alone.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One pane of a layout: either a program to run, or a split holding more
/// panes. A node with children ignores its own `command`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PaneDef {
    /// Shown on the pane's border.
    pub title: String,
    pub command: Vec<String>,
    /// `"5"` for lines or columns, `"30%"` for a share, empty to take what is
    /// left. Panes with no size split the remainder evenly.
    pub size: String,
    /// How this pane's children divide it: `"cols"` or `"rows"`.
    pub split: String,
    /// Where to run. Empty means the directory of whatever was focused when the
    /// layout was opened, which is almost always what a panel wants: a board or
    /// a repository browser is about the project you are in, not about your
    /// home directory. `~` is expanded.
    pub cwd: String,
    pub pane: Vec<PaneDef>,
}

/// A named arrangement of programs, listed above the projects in the sidebar
/// and opened as a space of its own.
///
/// A layout with a `command` and no `pane` entries is a single-program layout,
/// which is what the three defaults are. Nothing distinguishes it structurally
/// from the dashboard except how many leaves it has.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LayoutDef {
    pub name: String,
    /// Single key that jumps straight here.
    pub key: Option<char>,
    /// Used when there are no children.
    pub command: Vec<String>,
    pub split: String,
    pub pane: Vec<PaneDef>,
    /// What to say on the row without being opened.
    ///
    /// This is the difference between a link and an instrument: a dashboard you
    /// have to open to find out whether it matters is a link with extra steps.
    #[serde(default)]
    pub status: Option<StatusDef>,
    /// Whether the panes stay running when you look away.
    ///
    /// A monitoring dashboard keeps; a thing you opened for ten seconds should
    /// not sit there holding a lock. One boolean rather than a second concept:
    /// a separate "utility window" type would be two code paths that drift.
    pub keep: bool,
}

/// A command whose output becomes a badge on a board's row.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusDef {
    pub run: Vec<String>,
    /// How often, as a duration: `10s`, `2m`, or seconds on their own.
    #[serde(default = "ten_seconds")]
    pub every: String,
}

fn ten_seconds() -> String {
    "10s".into()
}

impl StatusDef {
    /// The interval, floored.
    ///
    /// Two seconds is the floor because this is a subprocess: four boards at
    /// ten seconds is already twenty-four processes a minute, and somebody
    /// writing `100ms` has not thought about what they are asking for.
    pub fn interval(&self) -> std::time::Duration {
        const FLOOR: std::time::Duration = std::time::Duration::from_secs(2);
        // Capped as well as floored: an interval of a century is a number, not
        // an intention, and the backoff multiplies whatever this returns.
        const CEILING: std::time::Duration = std::time::Duration::from_secs(24 * 3600);
        parse_every(&self.every)
            .unwrap_or(std::time::Duration::from_secs(10))
            .clamp(FLOOR, CEILING)
    }

    /// How long a run may take before it is killed.
    ///
    /// Its own interval, so a board asked every ten seconds may take ten -- but
    /// never less than a couple of seconds, because a slow disk is not a hang.
    pub fn cap(&self) -> std::time::Duration {
        self.interval().max(std::time::Duration::from_secs(2))
    }
}

/// `10s`, `2m`, `1h`, or a bare number of seconds.
pub fn parse_every(text: &str) -> Option<std::time::Duration> {
    let text = text.trim();
    let (digits, scale) = match text.chars().last()? {
        's' => (&text[..text.len() - 1], 1),
        'm' => (&text[..text.len() - 1], 60),
        'h' => (&text[..text.len() - 1], 3600),
        _ => (text, 1),
    };
    let n: u64 = digits.trim().parse().ok()?;
    Some(std::time::Duration::from_secs(n.saturating_mul(scale)))
}

impl PaneDef {
    /// This pane and its descendants, flattened to the ones that actually run
    /// something.
    fn leaves(&self) -> Vec<&PaneDef> {
        if self.pane.is_empty() {
            return vec![self];
        }
        self.pane.iter().flat_map(|p| p.leaves()).collect()
    }
}

impl LayoutDef {
    fn leaves(&self) -> Vec<&PaneDef> {
        if self.pane.is_empty() {
            return Vec::new();
        }
        self.pane.iter().flat_map(|p| p.leaves()).collect()
    }

    /// True when this layout can actually be built: every leaf names a program,
    /// and every one of those is installed.
    ///
    /// The first half matters as much as the second. `command` is optional in
    /// the file, so a pane can be written with a title and nothing to run, and
    /// a layout that is offered and then cannot start is worse than one that
    /// was never listed.
    pub fn runnable(&self) -> bool {
        let leaves = self.leaves();
        if leaves.is_empty() {
            // A single-program layout: the command lives on the layout itself.
            return self.command.first().is_some_and(|c| on_path(c));
        }
        leaves
            .iter()
            .all(|l| l.command.first().is_some_and(|c| on_path(c)))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Brand {
    #[serde(default = "default_mark")]
    pub mark: String,
    #[serde(default = "default_name")]
    pub name: String,
}

/// Empty means the glyph set's mark, so a terminal that cannot draw `▾` is not
/// handed a `◆` either. Naming one here makes it content, and content is yours.
fn default_mark() -> String {
    String::new()
}
fn default_name() -> String {
    "dirk".into()
}

impl Default for Brand {
    fn default() -> Self {
        Self {
            mark: default_mark(),
            name: default_name(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub brand: Brand,
    /// Where `o` looks for projects to open.
    pub projects_root: PathBuf,
    /// Named arrangements you jump to. `board` is the word the interface uses;
    /// `layout` is what it was called first and still parses.
    #[serde(rename = "board", alias = "layout")]
    pub layouts: Vec<LayoutDef>,
    /// Empty means `$SHELL`.
    pub shell: String,
    /// Wide enough for two-line rows: a name, an age beside it, and a branch
    /// under it. 28 was right when a row was a glyph and a word.
    pub sidebar_width: u16,
    pub scrollback: usize,
    pub naming: Naming,
    pub notify: Notify,
    pub sound: Sound,
    /// What to pipe a selection into. Empty means whatever this platform is
    /// likely to have -- `pbcopy`, `wl-copy`, `xclip`.
    pub clipboard: Vec<String>,
    pub nav: Nav,
    /// Harnesses dirk should recognise, on top of the ones it ships with.
    #[serde(rename = "agent")]
    pub agents: Vec<AgentDef>,
    /// Which agent `a` starts, when a project does not say.
    pub default_agent: String,
    /// Per-project settings. Everything here has a global answer too; this is
    /// where a dozen repositories stop wanting the same one.
    #[serde(rename = "project")]
    pub projects: Vec<ProjectDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDef {
    pub path: PathBuf,
    /// Which agent `a` starts here.
    #[serde(default)]
    pub agent: String,
    /// Whether this project may make a noise. The repository you are
    /// babysitting should be able to be quiet without silencing the one you
    /// are not.
    #[serde(default = "yes")]
    pub sound: bool,
}

/// A noise when an agent blocks, and a different one when it finishes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sound {
    /// Off unless asked for. It is an interruption, and one nobody chose is
    /// one they turn off rather than tune.
    pub enabled: bool,
    /// The command for each. Empty means the terminal's own bell, which needs
    /// no file and no player.
    pub blocked: Vec<String>,
    pub done: Vec<String>,
}

impl Sound {
    /// What to run for one of the two, after the shipped answer.
    pub fn command(&self, which: crate::sound::Alert) -> Vec<String> {
        let named = match which {
            crate::sound::Alert::Blocked => &self.blocked,
            crate::sound::Alert::Done => &self.done,
        };
        if !named.is_empty() {
            return named
                .iter()
                .map(|a| expand(a).to_string_lossy().into())
                .collect();
        }
        shipped(which)
    }
}

/// Two sounds that exist on the machine already, so this works with no file to
/// find and no configuration to write.
///
/// Distinguishable on purpose: one rises and one lands. Elsewhere there is no
/// set of sounds anybody can count on, and an empty command falls through to
/// the bell rather than to a player that may not be installed.
#[cfg(target_os = "macos")]
fn shipped(which: crate::sound::Alert) -> Vec<String> {
    let file = match which {
        crate::sound::Alert::Blocked => "/System/Library/Sounds/Funk.aiff",
        crate::sound::Alert::Done => "/System/Library/Sounds/Glass.aiff",
    };
    match std::path::Path::new(file).exists() {
        true => vec!["afplay".into(), file.into()],
        false => Vec::new(),
    }
}

#[cfg(not(target_os = "macos"))]
fn shipped(_which: crate::sound::Alert) -> Vec<String> {
    Vec::new()
}

/// One harness, as a configuration file describes it.
///
/// The shipped ones are expressed in exactly this shape, so the schema is
/// proven by the things already using it rather than by a second code path.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDef {
    pub name: String,
    /// Process names it runs under. Empty means its own name.
    #[serde(default)]
    pub names: Vec<String>,
    /// Fragments of a command line, for when it runs under an interpreter.
    #[serde(default)]
    pub argv: Vec<String>,
    /// How to start one. Empty means its own name.
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub blocked: BlockedDef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockedDef {
    /// Whether a marker only counts alongside a menu of numbered answers.
    #[serde(default = "yes")]
    pub menu: bool,
    #[serde(default, rename = "match")]
    pub markers: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for BlockedDef {
    fn default() -> Self {
        BlockedDef {
            menu: true,
            markers: Vec::new(),
        }
    }
}

/// How the column down the left is drawn.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Nav {
    /// Which shipped set of marks to draw with: `unicode` or `ascii`.
    pub glyphs: String,
    /// Marks to replace, for a font dirk cannot check the presence of.
    #[serde(rename = "glyph")]
    pub glyphs_override: Vec<GlyphDef>,
    /// Whether the attention zone holds its place: `when-needed`, `always` or
    /// `never`.
    pub attention: String,
    /// `tall` gives a workspace a second line for its branch; `short` gives it
    /// one line, which is what forty of them need.
    pub rows: String,
}

impl Default for Nav {
    fn default() -> Self {
        Nav {
            glyphs: "unicode".into(),
            glyphs_override: Vec::new(),
            attention: "when-needed".into(),
            rows: "tall".into(),
        }
    }
}

/// One mark, replaced.
///
/// `cells` is not optional and has no sensible default: it is how many columns
/// the terminal gives this text, which dirk cannot measure and must not guess.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlyphDef {
    pub name: String,
    pub text: String,
    pub cells: u16,
}

impl Config {
    /// The harnesses to recognise: the shipped ones, plus the file's.
    ///
    /// A block whose name matches a shipped one **replaces** it whole rather
    /// than merging field by field. Predictable beats clever -- somebody
    /// overriding claude's markers does not want to inherit half of ours -- and
    /// a merge would make "which markers am I actually using" unanswerable
    /// without reading two files.
    pub fn kinds(&self) -> Vec<crate::agent::Kind> {
        let mut out = crate::agent::defaults();
        for def in &self.agents {
            let kind = crate::agent::Kind {
                name: def.name.clone(),
                names: match def.names.is_empty() {
                    true => vec![def.name.clone()],
                    false => def.names.clone(),
                },
                argv: def.argv.clone(),
                command: match def.command.is_empty() {
                    true => vec![def.name.clone()],
                    false => def.command.clone(),
                },
                blocked: def.blocked.markers.clone(),
                choices: def.blocked.menu,
            };
            match out.iter().position(|k| k.name == kind.name) {
                Some(i) => out[i] = kind,
                None => out.push(kind),
            }
        }
        out
    }
}

impl Config {
    /// Which agent to start in a project: what it asks for, then the global
    /// answer, then nothing -- which means asking.
    ///
    /// Twelve repositories do not want one answer, and being asked the same
    /// question twelve times a day is how a shortcut stops being one.
    pub fn agent_for(&self, path: &std::path::Path) -> Option<String> {
        let named = self
            .projects
            .iter()
            .find(|p| expand(&p.path.to_string_lossy()) == path)
            .map(|p| p.agent.clone())
            .filter(|a| !a.is_empty());
        named
            .or_else(|| Some(self.default_agent.clone()))
            .filter(|a| !a.is_empty())
    }
}

impl Nav {
    /// The marks to draw with, after the overrides.
    pub fn glyphs(&self) -> crate::glyph::Glyphs {
        let mut set = crate::glyph::Glyphs::set(&self.glyphs);
        for def in &self.glyphs_override {
            if let Some(g) = crate::glyph::G::named(&def.name) {
                set.set_one(g, &def.text, def.cells);
            }
        }
        set
    }

    /// Whether the attention zone is drawn when it is empty, or at all.
    pub fn attention_always(&self) -> bool {
        self.attention == "always"
    }

    pub fn attention_never(&self) -> bool {
        self.attention == "never"
    }

    /// Whether a workspace gets a second line for its branch.
    pub fn tall(&self) -> bool {
        self.rows != "short"
    }
}

/// When dirk is allowed to interrupt you.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Notify {
    pub enabled: bool,
    /// Floor between two notifications about the same workspace. An agent that
    /// blocks, unblocks and blocks again inside a minute is one interruption.
    pub min_interval_ms: u64,
}

impl Default for Notify {
    fn default() -> Self {
        Self {
            enabled: true,
            min_interval_ms: 60_000,
        }
    }
}

/// The naming policy, as knobs.
///
/// Every default is what the policy did before it was configurable, so a file
/// that sets none of these changes nothing.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Naming {
    pub enabled: bool,
    /// How long a title must hold still before it is committed.
    pub debounce_ms: u64,
    /// Floor between two renames of one workspace.
    pub min_interval_ms: u64,
    /// Token overlap above which a new intent is "the same thing, reworded".
    pub similarity_threshold: f32,
    /// Never overwrite a name a human set. This is what stops the policy
    /// fighting you.
    pub respect_manual_names: bool,
    /// A blocked agent's title describes the dialog, not the work.
    pub skip_while_blocked: bool,
    /// Drop a leading project name, so a row under `ptop` does not read
    /// `ptop-adopt-remaining-lessons`.
    pub strip_project_prefix: bool,
    /// Titles that are programs rather than intents. Compared case-insensitively
    /// against the whole title.
    pub ignore_titles: Vec<String>,
    /// Mark an agent that has stopped revising its title. A signal for a
    /// human, never acted on.
    pub show_stale: bool,
    /// How many state transitions with no new intent count as stale.
    pub stale_after_turns: u32,
    pub targets: Targets,
    pub templates: Templates,
    pub sources: Sources,
}

/// Where an intent can come from.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Sources {
    pub llm: Llm,
}

/// A second source, for panes whose title says nothing.
///
/// Off unless asked for. The primary source is the agent's own terminal title,
/// which costs nothing and needs no key; this exists for the case that has no
/// title at all.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Llm {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    /// The *name* of the variable holding the key, never the key. A
    /// configuration file is a thing people paste into issues.
    pub api_key_env: String,
    pub timeout_ms: u64,
    /// How much of the screen to send, in characters.
    pub max_chars: usize,
    /// How many lines of the pane count as "the screen".
    pub viewport_lines: u16,
    /// Floor between two questions about one workspace. A naming call per turn
    /// per workspace, all day, for a caption, is not a trade anyone would make
    /// on purpose.
    pub interval_ms: u64,
}

impl Default for Llm {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "https://api.anthropic.com/v1/messages".into(),
            model: "claude-opus-5".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            // Generous: this is a non-streaming request to a model that thinks
            // before it answers, and a timeout is indistinguishable from "no
            // candidate" -- so one set too tight makes the feature quietly
            // never work.
            timeout_ms: 30_000,
            max_chars: 4_000,
            viewport_lines: 60,
            interval_ms: 600_000,
        }
    }
}

/// What naming is allowed to name.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Targets {
    pub workspace: bool,
    pub agent: bool,
}

impl Default for Targets {
    fn default() -> Self {
        Self {
            workspace: true,
            agent: true,
        }
    }
}

/// How each name is arranged, over the tokens in `tokens.rs`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Templates {
    pub workspace: String,
    pub agent: String,
}

impl Default for Templates {
    fn default() -> Self {
        Self {
            workspace: "{intent}".into(),
            agent: "{intent-slug}".into(),
        }
    }
}

/// Programs that show their own name as a terminal title. Without this list
/// they arrive as intents and become workspace labels.
fn default_ignore_titles() -> Vec<String> {
    [
        "claude",
        "claude code",
        "codex",
        "pi",
        "copilot",
        "cursor",
        "droid",
        "aider",
        "goose",
        "bash",
        "zsh",
        "fish",
        "sh",
        "nu",
        "pwsh",
        "powershell",
        "nvim",
        "vim",
        "nano",
        "helix",
        "hx",
        "emacs",
        "less",
        "man",
        "node",
        "python",
        "irb",
        "psql",
        "lazygit",
        "htop",
        "top",
        "btop",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

impl Default for Naming {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 1200,
            min_interval_ms: 15_000,
            similarity_threshold: 0.6,
            respect_manual_names: true,
            skip_while_blocked: true,
            strip_project_prefix: true,
            ignore_titles: default_ignore_titles(),
            show_stale: true,
            stale_after_turns: 6,
            targets: Targets::default(),
            templates: Templates::default(),
            sources: Sources::default(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            brand: Brand::default(),
            projects_root: home().join("Code"),
            layouts: default_layouts(),
            shell: String::new(),
            sidebar_width: 34,
            scrollback: 5000,
            naming: Naming::default(),
            notify: Notify::default(),
            sound: Sound::default(),
            clipboard: Vec::new(),
            nav: Nav::default(),
            agents: Vec::new(),
            default_agent: String::new(),
            projects: Vec::new(),
        }
    }
}

/// Three single-program layouts. One whose program is not installed is dropped
/// at startup rather than left to fail on first open — an entry that only ever
/// shows `command not found` is worse than no entry.
/// Written out rather than derived, because `#[serde(default = "yes")]` is a
/// *deserialization* default and does not reach `LayoutDef::default()` -- which
/// is how every shipped board was born with `keep: false` and shut itself down
/// the first time you looked at another one.
impl Default for LayoutDef {
    fn default() -> Self {
        LayoutDef {
            name: String::new(),
            key: None,
            command: Vec::new(),
            split: String::new(),
            pane: Vec::new(),
            status: None,
            keep: true,
        }
    }
}

fn default_layouts() -> Vec<LayoutDef> {
    let one = |name: &str, key: char, command: &[&str]| LayoutDef {
        name: name.into(),
        key: Some(key),
        command: command.iter().map(|s| (*s).to_string()).collect(),
        ..LayoutDef::default()
    };
    let pane = |title: &str, command: &[&str], size: &str| PaneDef {
        title: title.into(),
        command: command.iter().map(|s| (*s).to_string()).collect(),
        size: size.into(),
        ..PaneDef::default()
    };

    vec![
        // A dashboard, when the machine has the programs for one. It is
        // dropped like any other layout if they are not installed, so a fresh
        // install gets it without configuring anything and never gets a broken
        // version of it.
        LayoutDef {
            name: "Overview".into(),
            key: Some('1'),
            split: "rows".into(),
            pane: vec![
                pane("roadmap", &["cairn", "roadmap"], "10"),
                PaneDef {
                    split: "cols".into(),
                    pane: vec![
                        pane("ptop", &["ptop"], ""),
                        pane("board", &["cairn", "board"], "40%"),
                    ],
                    ..PaneDef::default()
                },
            ],
            ..LayoutDef::default()
        },
        one("ptop", '2', &["ptop"]),
        one("lazygit", '3', &["lazygit"]),
        one("cairn", '4', &["cairn", "board"]),
    ]
}

/// Expand a leading `~`, which is the only shell expansion a path in a config
/// file can reasonably expect.
pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// What is wrong with a configuration that still loads.
///
/// Separate from reading it because the same complaints are owed to whoever
/// asked, whether that is a terminal at startup or a client that said `reload`.
pub fn complaints(cfg: &Config) -> Vec<String> {
    let mut out = Vec::new();
    for l in &cfg.layouts {
        for bad in crate::mux::layout::bad_sizes(l) {
            out.push(format!(
                "layout {}: bad size {bad:?}; using an even share",
                l.name
            ));
        }
        // Command keys win, so a layout bound to one is unreachable. Silently
        // is the problem: the entry is listed with a key that does nothing.
        const RESERVED: &[char] = &[
            'n', 'o', 'a', 'x', 'r', 'v', 's', 'd', 'b', 'w', 'q', 'j', 'k', ';', '[', '/', 'z',
            '{', '}',
        ];
        if let Some(k) = l.key.filter(|k| RESERVED.contains(k)) {
            out.push(format!(
                "layout {}: key {k:?} is a command key and will not reach it",
                l.name
            ));
        }
        if !l.runnable() {
            out.push(format!("layout {}: nothing to run; dropped", l.name));
        }
    }
    // Layouts are matched to what is open by name, so a name used twice is one
    // layout you can open and one you cannot -- and nothing to say which.
    for (i, l) in cfg.layouts.iter().enumerate() {
        if cfg.layouts[..i].iter().any(|e| e.name == l.name) {
            out.push(format!(
                "layout {}: defined twice; only the first is reachable",
                l.name
            ));
        }
    }
    // An agent that can never match anything is a definition that does nothing,
    // and saying so is cheaper than wondering why a harness is never
    // recognised.
    for l in &cfg.layouts {
        if let Some(status) = &l.status {
            if status.run.is_empty() {
                out.push(format!("board {}: a status with nothing to run", l.name));
            }
            match parse_every(&status.every) {
                None => out.push(format!(
                    "board {}: every = {:?} is not a duration like \"10s\"",
                    l.name, status.every
                )),
                // Said rather than silently obeyed: a board somebody set to a
                // tenth of a second is not going to run at a tenth of a second,
                // and finding that out by measurement is worse than being told.
                Some(d) if d < std::time::Duration::from_secs(2) => out.push(format!(
                    "board {}: every = {:?} is below the two second floor",
                    l.name, status.every
                )),
                Some(_) => {}
            }
        }
    }
    for def in &cfg.agents {
        if def.names.is_empty() && def.argv.is_empty() && def.name.is_empty() {
            out.push("agent: an entry with no name, names or argv can never match".into());
        }
    }
    // A set that does not exist is drawn as the default one, and a mark that
    // does not exist is not drawn at all -- both silently, and both looking
    // exactly like a setting that did not take.
    const SETS: &[&str] = &["unicode", "ascii"];
    if !SETS.contains(&cfg.nav.glyphs.as_str()) {
        out.push(format!(
            "nav: no glyph set {:?}; using unicode. One of {}",
            cfg.nav.glyphs,
            SETS.join(", ")
        ));
    }
    for def in &cfg.nav.glyphs_override {
        if crate::glyph::G::named(&def.name).is_none() {
            out.push(format!("nav: no glyph called {:?}", def.name));
        }
    }
    for (field, value, allowed) in [
        (
            "attention",
            &cfg.nav.attention,
            &["when-needed", "always", "never"][..],
        ),
        ("rows", &cfg.nav.rows, &["tall", "short"][..]),
    ] {
        if !allowed.contains(&value.as_str()) {
            out.push(format!(
                "nav: {field} = {value:?} is not one of {}",
                allowed.join(", ")
            ));
        }
    }
    // A mistyped token renders as silence otherwise: a label simply shorter
    // than intended, with nothing to say why.
    for (what, template) in [
        ("workspace", &cfg.naming.templates.workspace),
        ("agent", &cfg.naming.templates.agent),
    ] {
        for token in crate::tokens::unknown(template) {
            out.push(format!("{what} template: no such token {{{token}}}"));
        }
    }
    out
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

impl Config {
    pub fn load() -> Self {
        let path = config_home().join("dirk").join("config.toml");
        let mut cfg = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("dirk: {}: {e}", path.display());
                Config::default()
            }),
            Err(_) => Config::default(),
        };
        // Reported here, before the terminal is taken over, because a message
        // printed after that is written onto the alternate screen and vanishes
        // with it.
        for line in complaints(&cfg) {
            eprintln!("dirk: {line}");
        }
        cfg.layouts.retain(|l| l.runnable());
        cfg
    }

    /// Read the file again and apply what can be applied to a running session.
    ///
    /// A file that does not parse is reported and the running configuration is
    /// kept: a typo should not cost you the session you were working in.
    ///
    /// Answers with whatever it had to say about the file. At startup those go
    /// to stderr, where there is a terminal to read them; here there is not,
    /// and a reload that quietly dropped a layout would be a reload that
    /// reported success for removing something.
    pub fn reload(
        current: &mut Config,
        session: &mut crate::mux::Session,
    ) -> Result<Vec<String>, String> {
        let path = config_home().join("dirk").join("config.toml");
        let next = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str::<Config>(&text).map_err(|e| e.to_string())?,
            // No file is a valid configuration: the defaults. Anything else is
            // a failure to read one that exists, and quietly installing the
            // defaults there would replace the layouts of somebody whose home
            // directory blinked.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut next = next;
        let said = complaints(&next);
        next.layouts.retain(|l| l.runnable());

        // Layouts that are open keep the panes they already have; the rest of
        // the list is replaced. Rebuilding an open dashboard because a colour
        // changed is not a reload, it is a restart.
        session.merge_layouts(&next.layouts);
        session.set_naming(&next.naming);
        // Cached at construction, so a reload that left them alone would report
        // success for a change that never reached anything. They take effect on
        // the next pane, which is what changing a shell means.
        session.shell = next.shell();
        session.set_scrollback(next.scrollback);
        *current = next;
        Ok(said)
    }

    pub fn shell(&self) -> String {
        if !self.shell.is_empty() {
            return self.shell.clone();
        }
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}

/// Is this program actually runnable? Absolute paths are checked directly;
/// bare names are looked up on PATH.
fn on_path(cmd: &str) -> bool {
    let p = Path::new(cmd);
    if p.is_absolute() || cmd.contains('/') {
        return p.is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(cmd).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Config {
        toml::from_str::<Config>(text).expect("parses")
    }

    #[test]
    fn a_board_keeps_its_panes_unless_it_says_otherwise() {
        // `#[serde(default = ...)]` is a deserialization default and does not
        // reach `LayoutDef::default()`, which is what `default_layouts` builds
        // every shipped board through. They were all born `keep: false` and
        // shut themselves down the first time you looked at another one.
        assert!(LayoutDef::default().keep, "the struct default disagrees");
        for board in default_layouts() {
            assert!(board.keep, "shipped board {} does not keep", board.name);
        }
        let named: Config =
            toml::from_str("[[board]]\nname = \"x\"\ncommand = [\"sh\"]\nkeep = false\n")
                .expect("parses");
        assert!(!named.layouts[0].keep, "an explicit false was ignored");
        let quiet: Config =
            toml::from_str("[[board]]\nname = \"y\"\ncommand = [\"sh\"]\n").expect("parses");
        assert!(
            quiet.layouts[0].keep,
            "a board that said nothing lost its panes"
        );
    }

    #[test]
    fn a_status_interval_is_a_duration_and_has_a_floor() {
        assert_eq!(parse_every("10s"), Some(std::time::Duration::from_secs(10)));
        assert_eq!(parse_every("2m"), Some(std::time::Duration::from_secs(120)));
        assert_eq!(
            parse_every("1h"),
            Some(std::time::Duration::from_secs(3600))
        );
        assert_eq!(parse_every("30"), Some(std::time::Duration::from_secs(30)));
        assert_eq!(parse_every("soon"), None);

        // Four boards at ten seconds is already twenty-four processes a
        // minute. Somebody writing a tenth of a second has not thought about
        // what they are asking for, and is told rather than obeyed.
        let quick = StatusDef {
            run: vec!["true".into()],
            every: "0s".into(),
        };
        assert_eq!(quick.interval(), std::time::Duration::from_secs(2));
        let mut cfg = Config::default();
        cfg.layouts.push(LayoutDef {
            name: "fast".into(),
            status: Some(quick),
            ..LayoutDef::default()
        });
        assert!(
            complaints(&cfg).iter().any(|c| c.contains("floor")),
            "an interval under the floor was accepted in silence"
        );
    }

    #[test]
    fn a_harness_from_the_file_is_recognised_like_a_shipped_one() {
        // The whole point: the field adds one a month, and an addition should
        // be an edit rather than a release.
        let cfg = parsed(
            r#"
            [[agent]]
            name = "sculptor"
            blocked = { menu = true, match = ["Proceed?"] }
            "#,
        );
        let kinds = cfg.kinds();
        let it = kinds.iter().find(|k| k.name == "sculptor").expect("added");
        // A name is enough: it stands for the process name and the command too.
        assert_eq!(it.names, ["sculptor"]);
        assert_eq!(it.command, ["sculptor"]);
        assert_eq!(
            crate::agent::identify(
                &crate::agent::Proc {
                    program: "sculptor".into(),
                    args: String::new(),
                },
                &kinds
            )
            .agent()
            .map(|k| k.name.as_str()),
            Some("sculptor")
        );
        // And the shipped ones are still there.
        assert!(kinds.iter().any(|k| k.name == "claude"));
    }

    #[test]
    fn overriding_a_shipped_harness_replaces_it_rather_than_merging() {
        // Predictable beats clever. Somebody replacing claude's markers does
        // not want to inherit half of ours, and a merge makes "which markers am
        // I actually using" unanswerable without reading two files.
        let cfg = parsed(
            r#"
            [[agent]]
            name = "claude"
            blocked = { menu = false, match = ["Ready:"] }
            "#,
        );
        let kinds = cfg.kinds();
        assert_eq!(
            kinds.iter().filter(|k| k.name == "claude").count(),
            1,
            "the shipped one was left beside the replacement"
        );
        let claude = kinds.iter().find(|k| k.name == "claude").unwrap();
        assert_eq!(claude.blocked, ["Ready:"]);
        assert!(!claude.choices);
    }

    #[test]
    fn a_harness_that_can_never_match_is_reported() {
        let mut cfg = Config::default();
        cfg.agents.push(AgentDef {
            name: String::new(),
            names: Vec::new(),
            argv: Vec::new(),
            command: Vec::new(),
            blocked: BlockedDef::default(),
        });
        assert!(
            complaints(&cfg).iter().any(|c| c.starts_with("agent:")),
            "an entry matching nothing was accepted in silence"
        );
    }
}
