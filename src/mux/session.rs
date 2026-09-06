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

//! The tree: projects → workspaces → panes, plus the layouts beside it.
//!
//! Two things live in the sidebar and they are not the same kind of thing.
//! **Layouts** are named arrangements with no project — one system monitor, one
//! dashboard — opened on demand and kept. **Projects** are directories you work
//! in, and a workspace is one unit of work inside one of them. That distinction
//! is why layouts sit above the tree rather than as a fourth level inside it.
//!
//! A layout *is* a workspace: panes, a split tree, a focused pane and a name is
//! the whole of one. So there is no separate code path for them, and the only
//! difference is where they are listed and that naming leaves their names
//! alone.
//!
//! A project appears here only once it has a workspace. A sidebar listing every
//! directory under `~/Code` would be a file browser; this is a list of what is
//! actually open, which is a different and much shorter list.

use crate::config::{Config, LayoutDef};
use crate::mux::layout;
use crate::mux::tree::{Dir, Node};
use crate::mux::{Ev, Pane, PaneId};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Instant;

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
    /// The panes themselves. The tree refers to them by id, so this is an arena
    /// rather than a layout.
    pub panes: Vec<Pane>,
    pub tree: Node,
    /// Which pane has the keyboard. An id rather than an index, because indices
    /// shift when a pane is removed and focus would silently move with them.
    pub focus: PaneId,
    pub naming: NameState,
}

impl Workspace {
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == id)
    }
    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == id)
    }

    pub fn active_pane(&self) -> Option<&Pane> {
        self.pane(self.focus)
    }
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.pane_mut(self.focus)
    }

    /// Where every pane goes, in draw order.
    pub fn rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        self.tree.rects(area)
    }

    /// Move focus to the next pane in draw order.
    pub fn cycle(&mut self) {
        let order = self.tree.leaves();
        if order.is_empty() {
            return;
        }
        let at = order.iter().position(|&id| id == self.focus).unwrap_or(0);
        self.focus = order[(at + 1) % order.len()];
    }

    /// Put focus on a pane that exists. Called after a removal, where the pane
    /// that had focus may be the one that went.
    fn refocus(&mut self) {
        if self.pane(self.focus).is_some() {
            return;
        }
        if let Some(&first) = self.tree.leaves().first() {
            self.focus = first;
        }
    }
}

pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub workspaces: Vec<Workspace>,
    pub expanded: bool,
}

pub struct Layout {
    pub def: LayoutDef,
    /// Built on first open, not at startup: five programs running all day so
    /// that one of them can be glanced at occasionally is a cost with nothing
    /// on the other side of it.
    pub ws: Option<Workspace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Layout(usize),
    Ws { p: usize, w: usize },
}

pub struct Session {
    pub layouts: Vec<Layout>,
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
            layouts: cfg
                .layouts
                .iter()
                .cloned()
                .map(|def| Layout { def, ws: None })
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
        let root = pane.id;
        proj.workspaces.push(Workspace {
            label: name,
            panes: vec![pane],
            tree: Node::Leaf(root),
            focus: root,
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

    /// Split the focused pane, putting a second shell beside it.
    pub fn split(&mut self, dir: Dir, rows: u16, cols: u16) {
        let Some(cwd) = self
            .focused_workspace()
            .and_then(|ws| ws.active_pane())
            .map(|x| x.cwd.clone())
        else {
            return;
        };
        let Some(pane) = self.spawn_shell(&cwd, rows, cols) else {
            return;
        };
        let Some(ws) = self.focused_workspace_mut() else {
            return;
        };

        let new = pane.id;
        let target = ws.focus;
        if !ws.tree.split(target, dir, new) {
            // The focused pane is not in the tree, which should be impossible.
            // Dropping the pane beats leaking a process nothing draws.
            return;
        }
        ws.panes.push(pane);
        ws.focus = new;
    }

    pub fn workspace(&self, p: usize, w: usize) -> Option<&Workspace> {
        self.projects.get(p)?.workspaces.get(w)
    }
    pub fn workspace_mut(&mut self, p: usize, w: usize) -> Option<&mut Workspace> {
        self.projects.get_mut(p)?.workspaces.get_mut(w)
    }

    /// The workspace that has the keyboard, whichever list it came from.
    ///
    /// A layout is a workspace, so answering `None` for one meant cycling panes
    /// and clicking to focus silently did nothing inside a dashboard — the one
    /// place with several panes to move between.
    pub fn focused_workspace(&self) -> Option<&Workspace> {
        match self.focus {
            Focus::Layout(i) => self.layouts.get(i)?.ws.as_ref(),
            Focus::Ws { p, w } => self.workspace(p, w),
        }
    }

    pub fn focused_workspace_mut(&mut self) -> Option<&mut Workspace> {
        match self.focus {
            Focus::Layout(i) => self.layouts.get_mut(i)?.ws.as_mut(),
            Focus::Ws { p, w } => self.workspace_mut(p, w),
        }
    }

    /// The pane keystrokes go to.
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        match self.focus {
            Focus::Layout(i) => self.layouts.get_mut(i)?.ws.as_mut()?.active_pane_mut(),
            Focus::Ws { p, w } => self.workspace_mut(p, w)?.active_pane_mut(),
        }
    }
    pub fn active_pane(&self) -> Option<&Pane> {
        match self.focus {
            Focus::Layout(i) => self.layouts.get(i)?.ws.as_ref()?.active_pane(),
            Focus::Ws { p, w } => self.workspace(p, w)?.active_pane(),
        }
    }

    // ── Layouts ─────────────────────────────────────────────────────────

    /// Focus a layout, building it if this is the first time.
    ///
    /// Built in three passes, because ids and geometry depend on each other:
    /// plan the tree, ask it where each pane will go, then spawn each program
    /// at the size it is actually getting. Spawning first and resizing after
    /// would show every program in the layout one redraw at the wrong size,
    /// which for a full-screen program is a visible flash.
    pub fn open_layout(&mut self, i: usize, area: Rect) {
        if self.layouts.get(i).is_none() {
            return;
        }
        if self.layouts[i].ws.is_none() {
            let def = self.layouts[i].def.clone();
            let base = self.next_id + 1;
            let (tree, leaves) = layout::plan(&def, base);
            self.next_id += leaves.len() as u64;

            let rects = tree.rects(area);
            let cwd = crate::config::home();
            let mut panes: Vec<Pane> = Vec::new();

            for (id, pane_def) in leaves {
                let r = rects
                    .iter()
                    .find(|(x, _)| *x == id)
                    .map(|(_, r)| *r)
                    .unwrap_or(area);
                let label = (!pane_def.title.is_empty()).then(|| pane_def.title.clone());
                // The label takes the pane's top row, so the program gets what
                // is left rather than what the tree allotted.
                let inner = content_of(label.is_some(), r);

                match Pane::spawn(
                    id,
                    &pane_def.command,
                    &cwd,
                    inner.height,
                    inner.width,
                    self.scrollback,
                    self.tx.clone(),
                ) {
                    Ok(mut p) => {
                        p.label = label;
                        panes.push(p);
                    }
                    Err(e) => {
                        // All or nothing: half a dashboard is not a dashboard,
                        // and the panes already started would be orphans.
                        eprintln!("dirk: {}: {e}", pane_def.command.join(" "));
                        return;
                    }
                }
            }

            let Some(&first) = tree.leaves().first() else {
                return;
            };
            self.layouts[i].ws = Some(Workspace {
                label: def.name.clone(),
                panes,
                tree,
                focus: first,
                naming: NameState::default(),
            });
        }
        self.focus = Focus::Layout(i);
    }

    // ── Lifecycle ───────────────────────────────────────────────────────

    /// A child exited. Drop its pane, and collapse anything left empty.
    ///
    pub fn reap(&mut self, id: PaneId) {
        for i in 0..self.layouts.len() {
            let Some(ws) = self.layouts[i].ws.as_mut() else {
                continue;
            };
            if let Some(k) = ws.panes.iter().position(|p| p.id == id) {
                ws.panes.remove(k);
                let empty = ws.tree.remove(id);
                ws.refocus();
                // A layout whose last pane exits goes back to unbuilt rather
                // than being removed: quitting lazygit should return you to the
                // tree and leave the entry there to be opened again.
                if empty || ws.panes.is_empty() {
                    self.layouts[i].ws = None;
                    if self.focus == Focus::Layout(i) {
                        self.focus = self.first_workspace().unwrap_or(Focus::Layout(i));
                    }
                }
                return;
            }
        }

        for p in 0..self.projects.len() {
            for w in 0..self.projects[p].workspaces.len() {
                let ws = &mut self.projects[p].workspaces[w];
                if let Some(k) = ws.panes.iter().position(|x| x.id == id) {
                    ws.panes.remove(k);
                    let empty = ws.tree.remove(id);
                    ws.refocus();
                    if empty || ws.panes.is_empty() {
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
            Focus::Layout(i) => self.layouts.get(i).is_some_and(|l| l.ws.is_some()),
            Focus::Ws { p, w } => self.workspace(p, w).is_some(),
        };
        if !ok {
            self.focus = self.first_workspace().unwrap_or(Focus::Layout(0));
        }
    }

    /// True when there is nothing left to show and dirk should exit.
    pub fn is_empty(&self) -> bool {
        self.projects.iter().all(|p| p.workspaces.is_empty())
            && self.layouts.iter().all(|l| l.ws.is_none())
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
        if let Some(ws) = self.focused_workspace_mut() {
            ws.cycle();
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
            Focus::Layout(_) => -1,
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
            Focus::Layout(i) => {
                if let Some(ws) = self.layouts.get_mut(i).and_then(|l| l.ws.as_mut()) {
                    resize_tree(ws, area);
                }
            }
            Focus::Ws { p, w } => {
                let Some(ws) = self.workspace_mut(p, w) else {
                    return;
                };
                resize_tree(ws, area);
            }
        }
    }
}

/// The part of a pane's rectangle that carries terminal content.
///
/// A labelled pane gives its top row to the label, so this is the one place
/// that arithmetic lives — drawing and resizing must agree about it or the
/// program inside is told a size it does not have.
pub fn content_of(labelled: bool, r: Rect) -> Rect {
    if !labelled || r.height < 2 {
        return r;
    }
    Rect {
        y: r.y + 1,
        height: r.height - 1,
        ..r
    }
}

fn resize_tree(ws: &mut Workspace, area: Rect) {
    for (id, r) in ws.tree.rects(area) {
        if let Some(pane) = ws.panes.iter_mut().find(|p| p.id == id) {
            let inner = content_of(pane.label.is_some(), r);
            pane.resize(inner.height, inner.width);
        }
    }
}
