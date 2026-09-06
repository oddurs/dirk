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
    /// Every program this pane or its descendants will run.
    fn programs(&self) -> Vec<&str> {
        if self.pane.is_empty() {
            return self
                .command
                .first()
                .map(|c| vec![c.as_str()])
                .unwrap_or_default();
        }
        self.pane.iter().flat_map(|p| p.programs()).collect()
    }
}

impl LayoutDef {
    pub fn programs(&self) -> Vec<&str> {
        if self.pane.is_empty() {
            return self
                .command
                .first()
                .map(|c| vec![c.as_str()])
                .unwrap_or_default();
        }
        self.pane.iter().flat_map(|p| p.programs()).collect()
    }

    /// True when every program this layout needs is installed. A layout is all
    /// or nothing: half a dashboard is not a dashboard.
    pub fn runnable(&self) -> bool {
        let programs = self.programs();
        !programs.is_empty() && programs.iter().all(|c| on_path(c))
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
    pub sidebar_width: u16,
    pub scrollback: usize,
    pub naming: Naming,
}

/// The namesync policy, as knobs.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Naming {
    pub enabled: bool,
    /// How long a title must hold still before it is committed as a name.
    pub debounce_ms: u64,
    /// Floor on how often one workspace may be renamed.
    pub min_interval_ms: u64,
}

impl Default for Naming {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 1200,
            min_interval_ms: 15_000,
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
            sidebar_width: 28,
            scrollback: 5000,
            naming: Naming::default(),
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
    vec![
        one("ptop", '1', &["ptop"]),
        one("lazygit", '2', &["lazygit"]),
        one("cairn", '3', &["cairn", "board"]),
    ]
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
        cfg.layouts.retain(|l| l.runnable());
        cfg
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
