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
use crate::git::{self, Repo};
use crate::mux::layout;
use crate::mux::tree::{Dir, Node};
use crate::mux::{Ev, Pane, PaneId};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

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
    /// What the agent in here is doing, as of the last update.
    pub state: crate::agent::State,
    /// Whether you have looked at this workspace since it last started work.
    ///
    /// The only thing separating `Done` from `Idle`, and the reason `Done`
    /// exists at all: finished work nobody has noticed is what is worth
    /// showing.
    pub seen: bool,
    /// When this workspace last produced output. The nav shows how long ago,
    /// because "how long has this been sitting there" is most of triage.
    pub touched: Instant,
    /// The panes themselves. The tree refers to them by id, so this is an arena
    /// rather than a layout.
    pub panes: Vec<Pane>,
    pub tree: Node,
    /// Whether the nav shows this workspace's panes. Collapsed by default: the
    /// tree should stay the height of the space list until it is asked for
    /// more.
    pub expanded: bool,
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
    /// What git last said. `None` means either not a repository or not asked
    /// yet, and the nav draws both the same way — as nothing.
    pub repo: Option<Repo>,
    /// When the answer arrived, so it can be asked again before it is wrong.
    pub read_at: Option<Instant>,
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
    /// The pane that had focus before a layout was opened, so closing one
    /// returns you exactly there.
    ///
    /// A pane id rather than `Focus`: `Focus::Ws` is a pair of indices, and
    /// `reap` removes workspaces and projects with `Vec::remove`, which shifts
    /// every index after them. Restoring by index lands you on whatever moved
    /// into the slot — exactly what "exactly where you were" promised not to do.
    previous: Option<PaneId>,
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
            previous: None,
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
            repo: None,
            read_at: None,
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
            state: crate::agent::State::None,
            seen: true,
            touched: Instant::now(),
            expanded: false,
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
            // A panel is about the project you are in. Falling back to the home
            // directory made `cairn board` report that there is no project
            // here, which is true of a home directory and useless as a panel.
            let here = self
                .focused_workspace()
                .and_then(|ws| ws.active_pane())
                .map(|p| p.cwd.clone())
                .unwrap_or_else(crate::config::home);
            let mut panes: Vec<Pane> = Vec::new();

            for (id, pane_def) in leaves {
                let r = rects
                    .iter()
                    .find(|(x, _)| *x == id)
                    .map(|(_, r)| *r)
                    .unwrap_or(area);
                let label = (!pane_def.title.is_empty()).then(|| pane_def.title.clone());
                let cwd = if pane_def.cwd.is_empty() {
                    here.clone()
                } else {
                    crate::config::expand(&pane_def.cwd)
                };
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
                state: crate::agent::State::None,
                seen: true,
                touched: Instant::now(),
                expanded: false,
                panes,
                tree,
                focus: first,
                naming: NameState::default(),
            });
        }
        if !matches!(self.focus, Focus::Layout(_)) {
            self.previous = self.active_pane().map(|p| p.id);
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
                // A layout is the arrangement a human named and asked for, so
                // it survives its contents exiting. The output is usually the
                // point -- a board that prints and exits is a reasonable panel,
                // and reaping it made that the panel dirk could least show.
                //
                // A pane the human closed is a different thing wearing the same
                // event, which is what `closing` distinguishes.
                if !ws.panes[k].closing {
                    ws.panes[k].finish();
                    return;
                }
                ws.panes.remove(k);
                let empty = ws.tree.remove(id);
                ws.refocus();
                // A layout whose last pane is closed goes back to unbuilt
                // rather than being removed: quitting lazygit should return you
                // to the tree and leave the entry there to be opened again.
                if empty || ws.panes.is_empty() {
                    self.layouts[i].ws = None;
                    if self.focus == Focus::Layout(i) {
                        // Back exactly where you were, found by identity so a
                        // workspace closing in the meantime cannot redirect it.
                        let back = self.previous.and_then(|id| self.holding(id));
                        self.focus = back
                            .or_else(|| self.first_workspace())
                            .unwrap_or(Focus::Layout(i));
                        self.previous = None;
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

    /// Which workspace holds this pane, if any still does.
    fn holding(&self, id: PaneId) -> Option<Focus> {
        for (p, proj) in self.projects.iter().enumerate() {
            for (w, ws) in proj.workspaces.iter().enumerate() {
                if ws.panes.iter().any(|x| x.id == id) {
                    return Some(Focus::Ws { p, w });
                }
            }
        }
        None
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
        let Some(pane) = self.active_pane_mut() else {
            return;
        };
        let (id, already_gone) = (pane.id, pane.dead);
        pane.close();
        // Killing a child that has already exited produces no new event, so a
        // stopped pane would sit there for ever waiting to be reaped by
        // something that was never going to arrive. This is the one case that
        // has to reap itself.
        if already_gone {
            self.reap(id);
        }
    }

    /// Start the focused pane's program again, in place.
    ///
    /// Only for a pane that has stopped. The tree keeps its shape, so the panel
    /// comes back where it was rather than the layout rearranging around it.
    pub fn restart_focused(&mut self, area: Rect) -> bool {
        let Some(ws) = self.focused_workspace() else {
            return false;
        };
        let id = ws.focus;
        let Some(old) = ws.pane(id) else { return false };
        if !old.dead {
            return false;
        }
        let (argv, cwd, label) = (old.argv.clone(), old.cwd.clone(), old.label.clone());
        let rect = ws
            .rects(area)
            .into_iter()
            .find(|(x, _)| *x == id)
            .map(|(_, r)| content_of(label.is_some(), r))
            .unwrap_or(area);

        let new_id = self.id();
        match Pane::spawn(
            new_id,
            &argv,
            &cwd,
            rect.height,
            rect.width,
            self.scrollback,
            self.tx.clone(),
        ) {
            Ok(mut pane) => {
                pane.label = label;
                let Some(ws) = self.focused_workspace_mut() else {
                    return false;
                };
                // Replace in place: same slot in the tree, same rectangle.
                ws.tree.replace(id, new_id);
                ws.panes.retain(|p| p.id != id);
                ws.panes.push(pane);
                ws.focus = new_id;
                true
            }
            Err(e) => {
                eprintln!("dirk: restart {}: {e}", argv.join(" "));
                false
            }
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

/// An agent that has been quiet for this long has stopped.
///
/// One that is thinking redraws its spinner continuously, so silence is the
/// signal rather than the absence of one. Long enough that a pause between two
/// lines of output does not read as finished, short enough that finishing is
/// noticed while you are still looking.
const WORKING_FOR: Duration = Duration::from_millis(1500);

/// What can be seen from outside, before the seen flag is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Observed {
    NoAgent,
    Blocked,
    Working,
    /// Stopped. Whether that is `Done` or `Idle` depends on you, not on it.
    Waiting,
}

fn observe(ws: &Workspace, now: Instant) -> Observed {
    let Some(pane) = ws.active_pane() else {
        return Observed::NoAgent;
    };
    if pane.dead {
        return Observed::NoAgent;
    }
    let Some(kind) = pane.occupant.agent() else {
        return Observed::NoAgent;
    };

    // Only panes holding an agent are read, which is what keeps this off the
    // cost of every tick.
    if let Ok(term) = pane.term.lock() {
        let screen = term.screen();
        let (rows, cols) = screen.size();
        // Anchored to the cursor, which is where a question is being asked.
        // Anchoring to the bottom of the screen only works once the screen is
        // full, and a program that has just started has all its output at the
        // top.
        let (cursor, _) = screen.cursor_position();
        let from = cursor.saturating_sub(crate::agent::PROMPT_ROWS);
        let prompt = screen.contents_between(from, 0, rows, cols);
        if crate::agent::is_blocked(kind, &prompt) {
            return Observed::Blocked;
        }
    }

    if now.duration_since(ws.touched) < WORKING_FOR {
        return Observed::Working;
    }
    Observed::Waiting
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

impl Session {
    /// Note that a pane produced output, so its workspace's age resets.
    pub fn touch(&mut self, id: PaneId) {
        let now = Instant::now();
        for proj in &mut self.projects {
            for ws in &mut proj.workspaces {
                if ws.panes.iter().any(|p| p.id == id) {
                    ws.touched = now;
                    return;
                }
            }
        }
    }

    /// Projects whose git answer is missing or old enough to ask again.
    ///
    /// Returned as paths because the read happens on another thread, and a
    /// project can be closed while its answer is still in flight.
    pub fn stale_repos(&mut self, now: Instant) -> Vec<PathBuf> {
        self.projects
            .iter_mut()
            .filter(|p| {
                p.read_at
                    .is_none_or(|t| now.duration_since(t) >= git::REFRESH)
            })
            .map(|p| {
                // Marked as asked before the answer arrives, or every tick
                // would start another read of the same directory.
                p.read_at = Some(now);
                p.path.clone()
            })
            .collect()
    }

    pub fn apply_repo(&mut self, answer: git::Answer) {
        if let Some(proj) = self.projects.iter_mut().find(|p| p.path == answer.dir) {
            proj.repo = answer.repo;
        }
    }
}

impl Session {
    /// Work out what every agent is doing.
    ///
    /// Run on the tick and on output, which is often enough that a state change
    /// is visible within a frame of happening and cheap because only panes
    /// holding an agent are read at all.
    pub fn update_states(&mut self, now: Instant) {
        let focus = self.focus;
        for p in 0..self.projects.len() {
            for w in 0..self.projects[p].workspaces.len() {
                let here = Focus::Ws { p, w };
                let observed = observe(&self.projects[p].workspaces[w], now);
                let ws = &mut self.projects[p].workspaces[w];
                apply(ws, observed, focus == here);
            }
        }
        for i in 0..self.layouts.len() {
            let here = Focus::Layout(i);
            let Some(ws) = self.layouts[i].ws.as_ref() else {
                continue;
            };
            let observed = observe(ws, now);
            let Some(ws) = self.layouts[i].ws.as_mut() else {
                continue;
            };
            apply(ws, observed, focus == here);
        }
    }
}

fn apply(ws: &mut Workspace, observed: Observed, focused: bool) {
    use crate::agent::State;

    // Looking at it is what "seen" means. Nothing else marks it, and reads over
    // the API must not -- otherwise a status line would clear your own
    // notifications by asking about them.
    if focused {
        ws.seen = true;
    } else if observed == Observed::Working && ws.state != State::Working {
        // Work has started that you have not watched. Whatever it produces is
        // unseen when it stops.
        ws.seen = false;
    }

    ws.state = match observed {
        Observed::NoAgent => State::None,
        Observed::Blocked => State::Blocked,
        Observed::Working => State::Working,
        Observed::Waiting if ws.seen => State::Idle,
        Observed::Waiting => State::Done,
    };
}

/// How long ago, coarsely.
///
/// Coarse on purpose: a per-second clock would redraw the nav constantly for a
/// value nobody reads that precisely, and the buckets change a handful of times
/// an hour.
pub fn since(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 45 {
        return "now".into();
    }
    let minutes = (secs + 30) / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

impl Session {
    /// Every live pane and the process group it currently has in the
    /// foreground, for a sample that will happen elsewhere.
    pub fn foregrounds(&self) -> Vec<(PaneId, Option<i32>)> {
        let mut out = Vec::new();
        let mut take = |ws: &Workspace| {
            for p in &ws.panes {
                match p.foreground() {
                    Some(pgid) => out.push((p.id, Some(pgid))),
                    // A pane with no foreground group has nothing running in
                    // it. Omitting it would leave it wearing whatever it was
                    // running when it died, for ever.
                    None => out.push((p.id, None)),
                }
            }
        };
        for l in &self.layouts {
            if let Some(ws) = &l.ws {
                take(ws);
            }
        }
        for proj in &self.projects {
            for ws in &proj.workspaces {
                take(ws);
            }
        }
        out
    }

    pub fn apply_agents(&mut self, reading: crate::agent::Reading) {
        let found: std::collections::HashMap<PaneId, Option<crate::agent::Occupant>> =
            reading.panes.into_iter().collect();
        let set = |ws: &mut Workspace| {
            for p in &mut ws.panes {
                match found.get(&p.id) {
                    // A miss is "no news", not "nothing running": the group can
                    // end between the pgid being read and `ps` running, which a
                    // shell does on every short command.
                    Some(None) | None => {}
                    Some(Some(o)) => p.occupant = o.clone(),
                }
            }
        };
        for i in 0..self.layouts.len() {
            if let Some(ws) = self.layouts[i].ws.as_mut() {
                set(ws);
            }
        }
        for p in 0..self.projects.len() {
            for w in 0..self.projects[p].workspaces.len() {
                set(&mut self.projects[p].workspaces[w]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::State;

    /// A workspace with only the fields the state machine reads.
    fn ws(state: State, seen: bool) -> Workspace {
        Workspace {
            label: String::new(),
            state,
            seen,
            touched: Instant::now(),
            panes: Vec::new(),
            tree: Node::Leaf(0),
            expanded: false,
            focus: 0,
            naming: NameState::default(),
        }
    }

    #[test]
    fn work_you_did_not_watch_finishes_as_done() {
        let mut w = ws(State::Working, false);
        apply(&mut w, Observed::Waiting, false);
        assert_eq!(w.state, State::Done);
    }

    #[test]
    fn work_you_watched_finishes_as_idle() {
        // Focused throughout, so there is nothing to be told about.
        let mut w = ws(State::Working, true);
        apply(&mut w, Observed::Waiting, true);
        assert_eq!(w.state, State::Idle);
    }

    #[test]
    fn looking_at_finished_work_settles_it() {
        let mut w = ws(State::Done, false);
        apply(&mut w, Observed::Waiting, true);
        assert_eq!(w.state, State::Idle, "focusing it is what marks it seen");
        // And it stays settled once looked at.
        apply(&mut w, Observed::Waiting, false);
        assert_eq!(w.state, State::Idle);
    }

    #[test]
    fn starting_work_out_of_sight_makes_the_next_stop_worth_reporting() {
        let mut w = ws(State::Idle, true);
        apply(&mut w, Observed::Working, false);
        assert_eq!(w.state, State::Working);
        assert!(!w.seen, "work started that you have not watched");
        apply(&mut w, Observed::Waiting, false);
        assert_eq!(w.state, State::Done);
    }

    #[test]
    fn starting_work_you_are_watching_does_not() {
        let mut w = ws(State::Idle, true);
        apply(&mut w, Observed::Working, true);
        assert!(w.seen);
        apply(&mut w, Observed::Waiting, true);
        assert_eq!(w.state, State::Idle);
    }

    #[test]
    fn blocked_outranks_everything_including_being_looked_at() {
        // It is the only state waiting on a human, so being watched does not
        // make it less true.
        let mut w = ws(State::Working, true);
        apply(&mut w, Observed::Blocked, true);
        assert_eq!(w.state, State::Blocked);
    }

    #[test]
    fn a_pane_with_no_agent_has_no_state() {
        let mut w = ws(State::Working, false);
        apply(&mut w, Observed::NoAgent, false);
        assert_eq!(w.state, State::None);
    }

    #[test]
    fn ages_are_bucketed_not_counted() {
        let s = |secs| since(Duration::from_secs(secs));
        assert_eq!(s(0), "now");
        assert_eq!(s(44), "now", "under a minute is not worth a number");
        assert_eq!(s(45), "1m");
        assert_eq!(s(90), "2m");
        assert_eq!(s(59 * 60), "59m");
        assert_eq!(s(60 * 60), "1h");
        assert_eq!(s(23 * 3600), "23h");
        assert_eq!(s(24 * 3600), "1d");
        assert_eq!(s(10 * 24 * 3600), "10d");
    }
}
