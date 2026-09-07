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
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LayoutDef {
    pub name: String,
    /// Single key that jumps straight here.
    pub key: Option<char>,
    /// Used when there are no children.
    pub command: Vec<String>,
    pub split: String,
    pub pane: Vec<PaneDef>,
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

fn default_mark() -> String {
    "◆".into()
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
    #[serde(rename = "layout")]
    pub layouts: Vec<LayoutDef>,
    /// Empty means `$SHELL`.
    pub shell: String,
    /// Wide enough for two-line rows: a name, an age beside it, and a branch
    /// under it. 28 was right when a row was a glyph and a word.
    pub sidebar_width: u16,
    pub scrollback: usize,
    pub naming: Naming,
    pub notify: Notify,
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
        }
    }
}

/// Three single-program layouts. One whose program is not installed is dropped
/// at startup rather than left to fail on first open — an entry that only ever
/// shows `command not found` is worse than no entry.
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
        for l in &cfg.layouts {
            for bad in crate::mux::layout::bad_sizes(l) {
                eprintln!(
                    "dirk: layout {}: bad size {bad:?}; using an even share",
                    l.name
                );
            }
        }
        // Command keys win, so a layout bound to one is unreachable. Silently
        // is the problem: the entry is listed with a key that does nothing.
        const RESERVED: &[char] = &['n', 'o', 'x', 'r', 'v', 's', 'd', 'w', 'q', 'j', 'k', ';'];
        for l in &cfg.layouts {
            if l.key.is_some_and(|k| RESERVED.contains(&k)) {
                eprintln!(
                    "dirk: layout {}: key {:?} is a command key and will not reach it",
                    l.name,
                    l.key.unwrap_or(' ')
                );
            }
        }
        // Reported before the terminal is taken over, where it can be read. A
        // mistyped token renders as silence otherwise: a label simply shorter
        // than intended, with nothing to say why.
        for (what, template) in [
            ("workspace", &cfg.naming.templates.workspace),
            ("agent", &cfg.naming.templates.agent),
        ] {
            for token in crate::tokens::unknown(template) {
                eprintln!("dirk: {what} template: no such token {{{token}}}");
            }
        }
        cfg.layouts.retain(|l| l.runnable());
        cfg
    }

    /// Read the file again and apply what can be applied to a running session.
    ///
    /// A file that does not parse is reported and the running configuration is
    /// kept: a typo should not cost you the session you were working in.
    pub fn reload(current: &mut Config, session: &mut crate::mux::Session) -> Result<(), String> {
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
        next.layouts.retain(|l| l.runnable());

        // Layouts that are open keep the panes they already have; the rest of
        // the list is replaced. Rebuilding an open dashboard because a colour
        // changed is not a reload, it is a restart.
        session.merge_layouts(&next.layouts);
        session.set_naming(&next.naming);
        *current = next;
        Ok(())
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
