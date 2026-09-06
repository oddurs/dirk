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

/// A static page: a program dirk keeps one of, above the projects in the
/// sidebar. Pages are panes like any other — the difference is that there is
/// exactly one, it is not attached to a project, and it is spawned the first
/// time you open it rather than at startup.
#[derive(Debug, Clone, Deserialize)]
pub struct PageDef {
    pub title: String,
    pub command: Vec<String>,
    /// Single key that jumps straight here.
    #[serde(default)]
    pub key: Option<char>,
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
    pub pages: Vec<PageDef>,
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
            pages: default_pages(),
            shell: String::new(),
            sidebar_width: 28,
            scrollback: 5000,
            naming: Naming::default(),
        }
    }
}

/// The three you asked for. A page whose program is not installed is dropped
/// at startup rather than left to fail on first open — a menu entry that only
/// ever shows `command not found` is worse than no entry.
fn default_pages() -> Vec<PageDef> {
    vec![
        PageDef {
            title: "ptop".into(),
            command: vec!["ptop".into()],
            key: Some('1'),
        },
        PageDef {
            title: "lazygit".into(),
            command: vec!["lazygit".into()],
            key: Some('2'),
        },
        PageDef {
            title: "cairn".into(),
            command: vec!["cairn".into(), "board".into()],
            key: Some('3'),
        },
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
        cfg.pages
            .retain(|p| p.command.first().is_some_and(|c| on_path(c)));
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
