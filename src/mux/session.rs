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

//! The tree: projects → workspaces → panes, plus the static pages beside it.
//!
//! Two things live in the sidebar and they are not the same kind of thing.
//! **Pages** are singletons with no project — one ptop, one lazygit, opened on
//! demand and kept. **Projects** are directories you work in, and a workspace
//! is one unit of work inside one of them. That distinction is why pages sit
//! above the tree rather than as a fourth level inside it.
//!
//! A project appears here only once it has a workspace. A sidebar listing every
//! directory under `~/Code` would be a file browser; this is a list of what is
//! actually open, which is a different and much shorter list.

use crate::config::{Config, PageDef};
use crate::mux::{Ev, Pane, PaneId};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Cols,
    Rows,
}

/// What `name.rs` needs to remember between titles.
#[derive(Debug, Default)]
pub struct NameState {
    /// The title currently settling, and when it first appeared.
    pub pending: Option<(String, Instant)>,
    pub last_rename: Option<Instant>,
    /// The last label dirk itself wrote. If the label differs from this, a
    /// human renamed it and dirk must not touch it again.
    pub applied: Option<String>,
}

pub struct Workspace {
    pub label: String,
    pub panes: Vec<Pane>,
    pub active: usize,
    pub split: Split,
    pub naming: NameState,
}

impl Workspace {
    pub fn active_pane(&self) -> Option<&Pane> {
        self.panes.get(self.active)
    }
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.active)
    }
}

pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub workspaces: Vec<Workspace>,
    pub expanded: bool,
}

pub struct Page {
    pub def: PageDef,
    /// Spawned on first open, not at startup: three programs running all day so
    /// that one of them can be glanced at is the cost smali's README already
    /// complained about.
    pub pane: Option<Pane>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Page(usize),
    Ws { p: usize, w: usize },
}

pub struct Session {
    pub pages: Vec<Page>,
    pub projects: Vec<Project>,
    pub focus: Focus,
    pub shell: String,
    scrollback: usize,
    next_id: u64,
    tx: Sender<Ev>,
}

impl Session {
    pub fn new(cfg: &Config, tx: Sender<Ev>) -> Self {
        Self {
            pages: cfg
                .pages
                .iter()
                .cloned()
                .map(|def| Page { def, pane: None })
                .collect(),
            projects: Vec::new(),
            focus: Focus::Ws { p: 0, w: 0 },
            shell: cfg.shell(),
            scrollback: cfg.scrollback,
            next_id: 1,
            tx,
        }
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    // ── Projects ────────────────────────────────────────────────────────

    /// Find the project for `path`, or add it. Returns its index.
    pub fn open_project(&mut self, path: &Path) -> usize {
        if let Some(i) = self.projects.iter().position(|p| p.path == path) {
            return i;
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.projects.push(Project {
            name,
            path: path.to_path_buf(),
            workspaces: Vec::new(),
            expanded: true,
        });
        self.projects.len() - 1
    }

    // ── Workspaces ──────────────────────────────────────────────────────

    /// Add a workspace to a project and focus it. The label starts as the
    /// project name; naming replaces it once the pane says what it is doing.
    pub fn new_workspace(&mut self, p: usize, rows: u16, cols: u16) -> Option<()> {
        let (path, name) = {
            let proj = self.projects.get(p)?;
            (proj.path.clone(), proj.name.clone())
        };
        let pane = self.spawn_shell(&path, rows, cols)?;
        let proj = self.projects.get_mut(p)?;
        proj.expanded = true;
        proj.workspaces.push(Workspace {
            label: name,
            panes: vec![pane],
            active: 0,
            split: Split::Cols,
            naming: NameState::default(),
        });
        self.focus = Focus::Ws {
            p,
            w: proj.workspaces.len() - 1,
        };
        Some(())
    }

    fn spawn_shell(&mut self, cwd: &Path, rows: u16, cols: u16) -> Option<Pane> {
        let id = self.id();
        let argv = vec![self.shell.clone()];
        match Pane::spawn(id, &argv, cwd, rows, cols, self.scrollback, self.tx.clone()) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("dirk: spawn {}: {e}", self.shell);
                None
            }
        }
    }

    /// Split the focused workspace, putting a second shell beside the first.
    pub fn split(&mut self, dir: Split, rows: u16, cols: u16) {
        let Focus::Ws { p, w } = self.focus else {
            return;
        };
        let Some(cwd) = self
            .workspace(p, w)
            .and_then(|ws| ws.active_pane())
            .map(|x| x.cwd.clone())
        else {
            return;
        };
        let Some(pane) = self.spawn_shell(&cwd, rows, cols) else {
            return;
        };
        if let Some(ws) = self.workspace_mut(p, w) {
            ws.split = dir;
            ws.panes.push(pane);
            ws.active = ws.panes.len() - 1;
        }
    }

    pub fn workspace(&self, p: usize, w: usize) -> Option<&Workspace> {
        self.projects.get(p)?.workspaces.get(w)
    }
    pub fn workspace_mut(&mut self, p: usize, w: usize) -> Option<&mut Workspace> {
        self.projects.get_mut(p)?.workspaces.get_mut(w)
    }

    pub fn focused_workspace_mut(&mut self) -> Option<&mut Workspace> {
        match self.focus {
            Focus::Ws { p, w } => self.workspace_mut(p, w),
            Focus::Page(_) => None,
        }
    }

    /// The pane keystrokes go to.
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        match self.focus {
            Focus::Page(i) => self.pages.get_mut(i)?.pane.as_mut(),
            Focus::Ws { p, w } => self.workspace_mut(p, w)?.active_pane_mut(),
        }
    }
    pub fn active_pane(&self) -> Option<&Pane> {
        match self.focus {
            Focus::Page(i) => self.pages.get(i)?.pane.as_ref(),
            Focus::Ws { p, w } => self.workspace(p, w)?.active_pane(),
        }
    }

    // ── Pages ───────────────────────────────────────────────────────────

    /// Focus a page, spawning it if this is the first time.
    pub fn open_page(&mut self, i: usize, rows: u16, cols: u16) {
        let Some(page) = self.pages.get(i) else {
            return;
        };
        if page.pane.is_none() {
            let argv = page.def.command.clone();
            let id = self.id();
            let cwd = crate::config::home();
            match Pane::spawn(
                id,
                &argv,
                &cwd,
                rows,
                cols,
                self.scrollback,
                self.tx.clone(),
            ) {
                Ok(p) => self.pages[i].pane = Some(p),
                Err(e) => {
                    eprintln!("dirk: spawn {}: {e}", argv.join(" "));
                    return;
                }
            }
        }
        self.focus = Focus::Page(i);
    }

    // ── Lifecycle ───────────────────────────────────────────────────────

    /// A child exited. Drop its pane, and collapse anything left empty.
    ///
    /// A page whose program exits goes back to unspawned rather than being
    /// removed: `q` in lazygit should return you to the tree and leave the
    /// entry there to be opened again.
    pub fn reap(&mut self, id: PaneId) {
        for (i, page) in self.pages.iter_mut().enumerate() {
            if page.pane.as_ref().is_some_and(|p| p.id == id) {
                page.pane = None;
                if self.focus == Focus::Page(i) {
                    self.focus = self.first_workspace().unwrap_or(Focus::Page(i));
                }
                return;
            }
        }

        for p in 0..self.projects.len() {
            for w in 0..self.projects[p].workspaces.len() {
                let ws = &mut self.projects[p].workspaces[w];
                if let Some(k) = ws.panes.iter().position(|x| x.id == id) {
                    ws.panes.remove(k);
                    ws.active = ws.active.min(ws.panes.len().saturating_sub(1));
                    if ws.panes.is_empty() {
                        self.projects[p].workspaces.remove(w);
                        if self.projects[p].workspaces.is_empty() {
                            self.projects.remove(p);
                        }
                        self.refocus();
                    }
                    return;
                }
            }
        }
    }

    fn first_workspace(&self) -> Option<Focus> {
        for (p, proj) in self.projects.iter().enumerate() {
            if !proj.workspaces.is_empty() {
                return Some(Focus::Ws { p, w: 0 });
            }
        }
        None
    }

    /// Put focus somewhere that exists.
    pub fn refocus(&mut self) {
        let ok = match self.focus {
            Focus::Page(i) => self.pages.get(i).is_some_and(|p| p.pane.is_some()),
            Focus::Ws { p, w } => self.workspace(p, w).is_some(),
        };
        if !ok {
            self.focus = self.first_workspace().unwrap_or(Focus::Page(0));
        }
    }

    /// True when there is nothing left to show and dirk should exit.
    pub fn is_empty(&self) -> bool {
        self.projects.iter().all(|p| p.workspaces.is_empty())
            && self.pages.iter().all(|p| p.pane.is_none())
    }

    /// Close the focused pane. Killing the child produces an `Exited` event,
    /// and `reap` does the structural work — so closing by hand and a program
    /// exiting on its own take exactly the same path.
    pub fn close_focused(&mut self) {
        if let Some(pane) = self.active_pane_mut() {
            pane.kill();
        }
    }

    /// Cycle focus through panes inside the focused workspace.
    pub fn cycle_pane(&mut self) {
        if let Some(ws) = self.focused_workspace_mut()
            && !ws.panes.is_empty()
        {
            ws.active = (ws.active + 1) % ws.panes.len();
        }
    }

    /// Every workspace in tree order, as `(project, workspace)` indices.
    pub fn flat(&self) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for (p, proj) in self.projects.iter().enumerate() {
            for w in 0..proj.workspaces.len() {
                v.push((p, w));
            }
        }
        v
    }

    /// Move focus to the next or previous workspace in tree order.
    pub fn step_workspace(&mut self, delta: isize) {
        let flat = self.flat();
        if flat.is_empty() {
            return;
        }
        let cur = match self.focus {
            Focus::Ws { p, w } => flat.iter().position(|&x| x == (p, w)).unwrap_or(0) as isize,
            Focus::Page(_) => -1,
        };
        let n = flat.len() as isize;
        let next = (cur + delta).rem_euclid(n) as usize;
        let (p, w) = flat[next];
        self.focus = Focus::Ws { p, w };
    }

    /// Push the current geometry down to every pane that is actually visible.
    /// Panes in unfocused workspaces are left at their old size until they are
    /// shown, so switching workspaces does not resize a tree of sleeping shells.
    pub fn resize_visible(&mut self, area: Rect) {
        match self.focus {
            Focus::Page(i) => {
                if let Some(pane) = self.pages.get_mut(i).and_then(|p| p.pane.as_mut()) {
                    pane.resize(area.height, area.width);
                }
            }
            Focus::Ws { p, w } => {
                let Some(ws) = self.workspace_mut(p, w) else {
                    return;
                };
                let rects = layout_panes(area, ws.panes.len(), ws.split);
                for (pane, r) in ws.panes.iter_mut().zip(rects) {
                    pane.resize(r.height, r.width);
                }
            }
        }
    }
}

/// Equal splits along one axis. Deliberately not a binary tree: a workspace is
/// a unit of work, and the moment it needs nested splits it wanted to be two
/// workspaces. Nested layouts are in the backlog, not in the way.
pub fn layout_panes(area: Rect, n: usize, split: Split) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let n16 = n as u16;
    (0..n16)
        .map(|i| match split {
            Split::Cols => {
                let w = area.width / n16;
                let x = area.x + w * i;
                let w = if i == n16 - 1 { area.width - w * i } else { w };
                Rect {
                    x,
                    y: area.y,
                    width: w,
                    height: area.height,
                }
            }
            Split::Rows => {
                let h = area.height / n16;
                let y = area.y + h * i;
                let h = if i == n16 - 1 { area.height - h * i } else { h };
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: h,
                }
            }
        })
        .collect()
}
