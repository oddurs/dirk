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
    /// The last label dirk itself wrote — the rendered one, not the intent it
    /// came from. If the live label differs from this, a human changed it.
    ///
    /// It has to be the rendered label: with a template, the intent and the
    /// label are different strings, and comparing the label against the intent
    /// makes every workspace look hand-written the moment it is named.
    pub applied: Option<String>,
    /// The last intent dirk named from.
    ///
    /// Separate from `applied` for the same reason: "has the intent changed" and
    /// "did a human edit this label" are different questions, and with a
    /// template they have different answers.
    pub last_intent: Option<String>,
    /// A human named this one and dirk has stood down.
    ///
    /// Held explicitly rather than worked out afresh each pass, so it can be
    /// shown in the interface and taken back. An inferred hold is one nobody
    /// can see or release.
    pub held: bool,
}

pub struct Workspace {
    /// A handle that does not move.
    ///
    /// The number beside a workspace in the nav is positional and changes when
    /// spaces are reordered; this does not. Anything addressing a workspace
    /// from outside uses it.
    pub id: u64,
    pub label: String,
    /// What the agent in here is doing, as of the last update.
    pub state: crate::agent::State,
    /// Whether you have looked at this workspace since it last started work.
    ///
    /// The only thing separating `Done` from `Idle`, and the reason `Done`
    /// exists at all: finished work nobody has noticed is what is worth
    /// showing.
    pub seen: bool,
    /// What dirk last interrupted you about here, and when.
    ///
    /// The state is half of it. One timestamp for the workspace meant that
    /// being told it was blocked suppressed being told it had finished, and
    /// `done` is a state that pushes no further change — so the second
    /// notification was not delayed, it was lost.
    pub notified: Option<(crate::agent::State, Instant)>,
    /// When the state last changed. `since` is measured from here, and answers
    /// "who has been blocked longest".
    pub state_since: Instant,
    /// When the intent last changed. `age` is measured from here, and answers
    /// "what has been grinding on the same thing all day". A workspace that has
    /// started and finished six times is still on one task, and this is the
    /// clock that says so.
    pub intent_since: Instant,
    /// The intent the clock above is measuring.
    pub intent: Option<String>,
    /// An intent from the second source, used only when the title has none.
    /// It enters the policy through the same door a title does.
    pub suggested: Option<String>,
    /// When the second source was last asked about this workspace.
    pub asked: Option<Instant>,
    /// State transitions since the intent last changed.
    ///
    /// Counted in transitions rather than in wall-clock time: an agent that has
    /// started and finished six times without revising what it says it is doing
    /// has either finished or is stuck, and how long that took is not the
    /// signal.
    pub turns: u32,
    /// When this workspace last produced output. The nav shows how long ago,
    /// because "how long has this been sitting there" is most of triage.
    pub touched: Instant,
    /// What the agent itself said it was doing, and when it said it.
    ///
    /// A report is a fact where everything else here is a guess, so it wins.
    /// It is not a lease and does not expire on a clock -- a reported `done`
    /// has to survive until you look at it or the seen rule means nothing.
    /// Two things end one: the next report, and output, which contradicts a
    /// claim that nothing is happening.
    pub reported: Option<(crate::agent::State, Instant)>,
    /// What decided the current state. Reported for the sake of being able to
    /// explain a badge that is wrong.
    pub source: crate::agent::Source,
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
    /// Transitions with no new intent that count as stale. Zero is off.
    stale_after: u32,
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
            stale_after: if cfg.naming.show_stale {
                cfg.naming.stale_after_turns
            } else {
                0
            },
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
        let id = self.id();
        let proj = self.projects.get_mut(p)?;
        proj.expanded = true;
        let root = pane.id;
        proj.workspaces.push(Workspace {
            id,
            label: name,
            state: crate::agent::State::None,
            seen: true,
            notified: None,
            state_since: Instant::now(),
            intent_since: Instant::now(),
            intent: None,
            suggested: None,
            asked: None,
            turns: 0,
            touched: Instant::now(),
            reported: None,
            source: crate::agent::Source::None,
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
            let ws_id = self.id();
            self.layouts[i].ws = Some(Workspace {
                id: ws_id,
                label: def.name.clone(),
                state: crate::agent::State::None,
                seen: true,
                notified: None,
                state_since: Instant::now(),
                intent_since: Instant::now(),
                intent: None,
                suggested: None,
                asked: None,
                turns: 0,
                touched: Instant::now(),
                reported: None,
                source: crate::agent::Source::None,
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

    pub fn first_workspace(&self) -> Option<Focus> {
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

    /// Shut a board's panes down, so a board that does not keep them does not.
    ///
    /// Everything a board is made of is a running program, and one you opened
    /// for ten seconds should not sit there afterwards holding a lock. Only for
    /// boards that asked: the default is still that an open board keeps what is
    /// running in it.
    pub fn close_layout(&mut self, i: usize) {
        let Some(layout) = self.layouts.get_mut(i) else {
            return;
        };
        let Some(ws) = layout.ws.take() else {
            return;
        };
        for mut pane in ws.panes {
            pane.close();
        }
        self.refocus();
    }

    /// Move the focused pane's view through its scrollback.
    ///
    /// Positive is towards the past. Answers where it ended up, so the caller
    /// can tell a scroll that did something from one that hit the end.
    pub fn scroll_focused(&mut self, delta: isize) -> usize {
        let Some(pane) = self.active_pane_mut() else {
            return 0;
        };
        let want = (pane.scroll as isize).saturating_sub(-delta).max(0) as usize;
        let Ok(mut term) = pane.term.lock() else {
            return pane.scroll;
        };
        // vt100 clamps to what scrollback there actually is, so the answer to
        // "how far back am I" comes from it rather than from what was asked.
        term.screen_mut().set_scrollback(want);
        pane.scroll = term.screen().scrollback();
        pane.scroll
    }

    /// Back to the live screen. What typing means.
    pub fn unscroll_focused(&mut self) {
        let Some(pane) = self.active_pane_mut() else {
            return;
        };
        if pane.scroll == 0 {
            return;
        }
        pane.scroll = 0;
        if let Ok(mut term) = pane.term.lock() {
            term.screen_mut().set_scrollback(0);
        }
    }

    /// How far back the focused pane is being read.
    pub fn scrolled(&self) -> usize {
        self.active_pane().map_or(0, |p| p.scroll)
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

/// A state that changed, for whoever wants to say so out loud.
#[derive(Debug, Clone)]
pub struct Change {
    pub at: Focus,
    pub label: String,
    pub to: crate::agent::State,
    pub focused: bool,
}

/// The last `lines` a pane has written, as text.
///
/// Anchored to the cursor, not to the bottom of the grid. A screen that has not
/// filled yet has all its output at the top and nothing at the bottom, so
/// reading the last rows of the grid answers with blank lines — which is what
/// `pane read` did, and what the blocked check did before it was fixed for the
/// same reason.
fn viewport(pane: &Pane, lines: u16) -> Option<String> {
    let term = pane.term.lock().ok()?;
    let screen = term.screen();
    let (rows, cols) = screen.size();
    let last = rows.saturating_sub(1);
    let (cursor, _) = screen.cursor_position();

    // The lower of the two is not enough. A full-screen program that homes its
    // cursor after a repaint -- `top`, `less`, `vim` on line one -- would give
    // a row index near zero and answer with two rows of a full screen. The
    // furthest down anything has been written is the other candidate.
    let written = (0..rows)
        .rev()
        .find(|&r| (0..cols).any(|c| screen.cell(r, c).is_some_and(|x| x.has_contents())))
        .unwrap_or(0);
    let to = cursor.max(written).min(last);
    // `contents_between` is inclusive, so `lines` rows means `lines - 1` back.
    let from = to.saturating_sub(lines.saturating_sub(1));
    Some(screen.contents_between(from, 0, to, cols))
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
    /// The agent said so. Carries what it said, and it is not second-guessed.
    Said(crate::agent::State),
}

fn observe(ws: &Workspace, now: Instant) -> Observed {
    let Some(pane) = ws.active_pane() else {
        return Observed::NoAgent;
    };
    if pane.dead {
        return Observed::NoAgent;
    }

    // Rank one, and the only source here that is not a guess. Taken before the
    // occupant is even consulted: `agent start` reports `starting` for a pane
    // whose process has not appeared in the table yet, and that gap is exactly
    // the moment the state is most worth having.
    //
    // Output ends it, whatever it said. A report is a claim about a moment, and
    // the moment is over as soon as the pane says something the report did not
    // account for -- which is true in both directions and not only of a claim
    // that nothing is happening. A `blocked` that outlived the answer you typed
    // pins the workspace to the top of the attention zone until the next hook
    // fires; a `working` whose agent was interrupted sticks for ever and, worse,
    // forces `seen` false on every tick so it can never be cleared.
    //
    // Nothing is lost by being strict here: inference runs on the very next
    // line, and if the agent really is still blocked its own prompt is on the
    // screen for the screen rule to find.
    if let Some((said, at)) = ws.reported
        && pane.touched <= at
    {
        return Observed::Said(said);
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
        // Bounded on both sides. `contents_between` takes row indices, not a
        // count, and passing the count made the end unreachable -- so the read
        // ran to the bottom of the screen and the window was not around the
        // cursor at all, which is the whole idea.
        let last = rows.saturating_sub(1);
        let to = cursor.min(last);
        let from = to.saturating_sub(crate::agent::PROMPT_ROWS);
        let prompt = screen.contents_between(from, 0, to, cols);
        if crate::agent::is_blocked(kind, &prompt) {
            return Observed::Blocked;
        }
    }

    if now.duration_since(pane.touched) < WORKING_FOR {
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
    /// Note that a pane produced output.
    ///
    /// Both the pane's own clock and the workspace's: the pane's decides
    /// whether its agent is working, and the workspace's is the age in the nav,
    /// which is about the workspace as a whole.
    ///
    /// Deliberately does *not* return a scrolled pane to the bottom. Being
    /// scrolled back is a thing you asked for, and a build that prints a line
    /// every second would otherwise drag you out of what you were reading --
    /// which is the reason to be reading it. Typing is what brings you back,
    /// because typing is a statement about the live screen.
    pub fn touch(&mut self, id: PaneId) {
        let now = Instant::now();
        let mark = |ws: &mut Workspace| -> bool {
            let Some(pane) = ws.panes.iter_mut().find(|p| p.id == id) else {
                return false;
            };
            pane.touched = now;
            ws.touched = now;
            true
        };
        for proj in &mut self.projects {
            for ws in &mut proj.workspaces {
                if mark(ws) {
                    return;
                }
            }
        }
        // Layouts were missed here, so a layout's clock was frozen at creation
        // and an agent in one could never be seen working.
        for layout in &mut self.layouts {
            if let Some(ws) = layout.ws.as_mut()
                && mark(ws)
            {
                return;
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
    pub fn update_states(&mut self, now: Instant) -> Vec<Change> {
        let mut changes = Vec::new();
        let focus = self.focus;
        for p in 0..self.projects.len() {
            for w in 0..self.projects[p].workspaces.len() {
                let here = Focus::Ws { p, w };
                let observed = observe(&self.projects[p].workspaces[w], now);
                let ws = &mut self.projects[p].workspaces[w];
                let was = ws.state;
                apply(ws, observed, focus == here);
                if ws.state != was {
                    changes.push(Change {
                        at: here,
                        label: ws.label.clone(),
                        to: ws.state,
                        focused: focus == here,
                    });
                }
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
        changes
    }

    /// Note the intent a workspace's agent is publishing, so `age` can measure
    /// how long it has been the same one.
    pub fn track_intents(&mut self, cfg: &crate::config::Naming) {
        for proj in &mut self.projects {
            for ws in &mut proj.workspaces {
                let title = ws.active_pane().and_then(|p| p.title());
                let now = crate::name::normalize(title.as_deref().unwrap_or_default());
                // A title the policy would reject is not an intent. Without
                // this, a pane whose title is a shell prompt or the agent's own
                // name counts as having one -- and those are exactly the panes
                // the second source exists for, so they would never be asked
                // about and would keep their project name for ever.
                let now = (!now.is_empty()
                    && !crate::name::is_junk(&now, &proj.name, &cfg.ignore_titles))
                .then_some(now);
                if now != ws.intent {
                    // Only when it actually changed. A title republished
                    // unchanged is the same intent, and `age` is what says how
                    // long that has been true.
                    ws.intent_since = Instant::now();
                    ws.intent = now;
                    ws.turns = 0;
                }
            }
        }
    }

    /// Workspaces whose title says nothing, with what is on their screen.
    ///
    /// Only panes holding an agent, only when there is no title to work from,
    /// and only past the floor between questions — the second source exists for
    /// the case the first cannot cover, not as a second opinion on it.
    pub fn wants_intent(
        &mut self,
        cfg: &crate::config::Llm,
        now: Instant,
    ) -> Vec<(PaneId, String)> {
        let floor = Duration::from_millis(cfg.interval_ms);
        let mut out = Vec::new();
        for proj in &mut self.projects {
            for ws in &mut proj.workspaces {
                if ws.intent.is_some() {
                    continue;
                }
                // A workspace holding two agents has no single intent, so the
                // policy will refuse whatever comes back. Asking would be paid
                // for and discarded.
                if ws.panes.len() > 1 {
                    continue;
                }
                if ws.asked.is_some_and(|t| now.duration_since(t) < floor) {
                    continue;
                }
                // Read and released before the workspace is written to.
                let asked = match ws.active_pane() {
                    Some(pane) if pane.occupant.agent().is_some() && !pane.dead => {
                        viewport(pane, cfg.viewport_lines)
                            .filter(|s| !s.trim().is_empty())
                            .map(|screen| (pane.id, screen))
                    }
                    _ => None,
                };
                let Some(asked) = asked else { continue };
                // Marked before the answer arrives, or every tick would
                // start another question about the same workspace.
                ws.asked = Some(now);
                out.push(asked);
            }
        }
        out
    }

    pub fn apply_suggestion(&mut self, pane: PaneId, intent: String) {
        for proj in &mut self.projects {
            for ws in &mut proj.workspaces {
                if ws.panes.iter().any(|p| p.id == pane) {
                    ws.suggested = Some(intent);
                    return;
                }
            }
        }
    }

    /// Take a new list of layouts without disturbing the ones that are open.
    ///
    /// An open layout keeps its panes. Rebuilding a running dashboard because a
    /// colour changed elsewhere in the file is not a reload, it is a restart.
    pub fn merge_layouts(&mut self, next: &[crate::config::LayoutDef]) {
        // Focus into a layout is an index into this vector, and the new file
        // may order them differently. Remembered by name across the merge, or a
        // reload could leave you looking at -- and typing into -- a different
        // layout's panes.
        let focused = match self.focus {
            Focus::Layout(i) => self.layouts.get(i).map(|l| l.def.name.clone()),
            Focus::Ws { .. } => None,
        };
        let mut kept: Vec<Layout> = Vec::new();
        for def in next {
            let open = self
                .layouts
                .iter_mut()
                .find(|l| l.def.name == def.name && l.ws.is_some())
                .and_then(|l| l.ws.take());
            kept.push(Layout {
                def: def.clone(),
                ws: open,
            });
        }
        // A layout that has gone from the file but is open stays until it is
        // closed: its panes are running programs, and a reload should not kill
        // them.
        for old in self.layouts.drain(..) {
            if old.ws.is_some() && !kept.iter().any(|l| l.def.name == old.def.name) {
                kept.push(old);
            }
        }
        self.layouts = kept;
        if let Some(name) = focused {
            match self.layouts.iter().position(|l| l.def.name == name) {
                Some(i) => self.focus = Focus::Layout(i),
                // Its definition has gone and it was not open, so there is
                // nothing left to look at.
                None => self.focus = self.first_workspace().unwrap_or(Focus::Layout(0)),
            }
        }
        self.refocus();
    }

    /// Take the naming knobs that a running session reads directly.
    /// How much history a new pane keeps. Existing panes keep what they have:
    /// a scrollback is what has already been said, and shortening it would
    /// throw away the part of the session you kept it for.
    pub fn set_scrollback(&mut self, lines: usize) {
        self.scrollback = lines;
    }

    pub fn set_naming(&mut self, cfg: &crate::config::Naming) {
        self.stale_after = if cfg.show_stale {
            cfg.stale_after_turns
        } else {
            0
        };
    }

    /// Type into a pane, wherever it is.
    pub fn write_to(&mut self, id: PaneId, bytes: &[u8]) -> bool {
        match self.pane_anywhere_mut(id) {
            Some(pane) if !pane.dead => {
                pane.write(bytes);
                true
            }
            _ => false,
        }
    }

    /// Close one pane by handle, wherever it is.
    pub fn close_pane(&mut self, id: PaneId) -> bool {
        let Some(pane) = self.pane_anywhere_mut(id) else {
            return false;
        };
        let gone = pane.dead;
        pane.close();
        if gone {
            // Killing a child that has already exited emits no event, so this
            // is the one case that has to reap itself.
            self.reap(id);
        }
        true
    }

    /// The last `lines` of what a pane has drawn, wherever it is.
    ///
    /// What an agent asking about a neighbour actually wants: the screen, not
    /// the scrollback, and not an escape-sequence stream it would have to parse.
    pub fn pane_text(&self, id: PaneId, lines: u16) -> Option<String> {
        for ws in self.every_workspace() {
            if let Some(pane) = ws.pane(id) {
                return viewport(pane, lines);
            }
        }
        None
    }

    /// Everything a name can be made of, for one workspace.
    pub fn tokens(&self, p: usize, w: usize) -> crate::tokens::Tokens {
        use crate::tokens::Tokens;
        let mut t = Tokens::default();
        let Some(proj) = self.projects.get(p) else {
            return t;
        };
        let Some(ws) = proj.workspaces.get(w) else {
            return t;
        };

        t.set("project", proj.name.clone());
        if let Some(repo) = &proj.repo {
            t.set("branch", repo.branch.clone());
            t.flag("worktree", "worktree", repo.worktree);
        }

        // Positional, and it changes when spaces are reordered -- display
        // rather than identity.
        let n = self.flat().iter().position(|&x| x == (p, w)).map(|i| i + 1);
        if let Some(n) = n {
            t.set("n", n.to_string());
        }

        if let Some(intent) = &ws.intent {
            t.set("intent", intent.clone());
            t.set(
                "intent-slug",
                crate::name::slugify(intent, crate::name::AGENT_NAME_MAX),
            );
        }

        // Two clocks. `since` restarts on every state change; `age` only when
        // the intent does.
        t.set("since", since(ws.state_since.elapsed()));
        t.set("age", since(ws.intent_since.elapsed()));

        if let Some(pane) = ws.active_pane()
            && let Some(kind) = pane.occupant.agent()
        {
            t.set("agent", &kind.name);
        }
        let agents = ws
            .panes
            .iter()
            .filter(|p| p.occupant.agent().is_some())
            .count();
        if agents > 1 {
            t.set("agents", agents.to_string());
        }

        t.flag("locked", "held", ws.naming.held);
        t.flag("stale", "stale", self.is_stale(ws));
        t
    }

    /// Has this agent stopped saying anything new?
    ///
    /// A signal for a human and never acted on: nothing is renamed, skipped or
    /// notified because of it.
    pub fn is_stale(&self, ws: &Workspace) -> bool {
        self.stale_after > 0 && ws.intent.is_some() && ws.turns >= self.stale_after
    }

    /// Release the hold on the focused workspace, so naming may claim it again.
    ///
    /// Reachable without knowing a command, because a hold nobody can release
    /// is a workspace stuck with a name for ever.
    pub fn release_hold(&mut self) -> bool {
        let Some(ws) = self.focused_workspace_mut() else {
            return false;
        };
        if !ws.naming.held {
            return false;
        }
        ws.naming.held = false;
        // Adopted, not forgotten. Clearing this makes the very next pass see a
        // label dirk did not write and take the hold straight back -- so `u`
        // would say "released" and release nothing.
        ws.naming.applied = Some(ws.label.clone());
        ws.naming.last_intent = None;
        true
    }

    /// Give every agent a name, and take it back when the agent goes.
    ///
    /// Suggested from the intent of the workspace it is in, which is the same
    /// thing its label comes from, so `dirk agent send-keys mux-core` names
    /// something you would recognise. Unique among live agents, because that is
    /// what a name is for.
    pub fn name_agents(&mut self, cfg: &crate::config::Naming) {
        if !cfg.targets.agent {
            return;
        }
        let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
        // Existing names are kept, so a name does not move under an agent that
        // is still running.
        for ws in self.every_workspace() {
            for pane in &ws.panes {
                if pane.occupant.agent().is_some()
                    && let Some(n) = &pane.agent_name
                {
                    taken.insert(n.clone());
                }
            }
        }

        let mut assign: Vec<(PaneId, Option<String>)> = Vec::new();
        for ws in self.every_workspace() {
            for pane in &ws.panes {
                match (pane.occupant.agent(), &pane.agent_name) {
                    // Already named, and still an agent.
                    (Some(_), Some(_)) => {}
                    (Some(kind), None) => {
                        let from = crate::name::slugify(&ws.label, crate::name::AGENT_NAME_MAX);
                        let from = if from.is_empty() {
                            kind.name.to_string()
                        } else {
                            from
                        };
                        let name = crate::name::unique(&from, &taken);
                        if !name.is_empty() {
                            taken.insert(name.clone());
                            assign.push((pane.id, Some(name)));
                        }
                    }
                    // Not an agent any more; the name is cleared rather than
                    // left on a shell for the next agent to collide with.
                    (None, Some(_)) => assign.push((pane.id, None)),
                    (None, None) => {}
                }
            }
        }

        for (id, name) in assign {
            if let Some(pane) = self.pane_anywhere_mut(id) {
                pane.agent_name = name;
            }
        }
    }

    fn every_workspace(&self) -> impl Iterator<Item = &Workspace> {
        self.layouts
            .iter()
            .filter_map(|l| l.ws.as_ref())
            .chain(self.projects.iter().flat_map(|p| p.workspaces.iter()))
    }

    fn pane_anywhere_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        for layout in &mut self.layouts {
            if let Some(ws) = layout.ws.as_mut()
                && let Some(p) = ws.panes.iter_mut().find(|p| p.id == id)
            {
                return Some(p);
            }
        }
        for proj in &mut self.projects {
            for ws in &mut proj.workspaces {
                if let Some(p) = ws.panes.iter_mut().find(|p| p.id == id) {
                    return Some(p);
                }
            }
        }
        None
    }

    /// The workspace in a given state that has been in it longest.
    ///
    /// Where the counts in the rail jump to: the thing that has been waiting
    /// longest is the thing to look at first.
    pub fn oldest_in(&self, state: crate::agent::State) -> Option<Focus> {
        self.projects
            .iter()
            .enumerate()
            .flat_map(|(p, proj)| {
                proj.workspaces
                    .iter()
                    .enumerate()
                    .map(move |(w, ws)| (p, w, ws))
            })
            .filter(|(_, _, ws)| ws.state == state)
            .max_by_key(|(_, _, ws)| ws.touched.elapsed())
            .map(|(p, w, _)| Focus::Ws { p, w })
    }

    /// How many workspaces are blocked, and how many have finished unseen.
    pub fn counts(&self) -> (usize, usize) {
        let mut blocked = 0;
        let mut done = 0;
        for proj in &self.projects {
            for ws in &proj.workspaces {
                match ws.state {
                    crate::agent::State::Blocked => blocked += 1,
                    crate::agent::State::Done => done += 1,
                    _ => {}
                }
            }
        }
        (blocked, done)
    }

    /// Whether this workspace may be interrupted about, noting that it was.
    ///
    /// An agent that blocks, unblocks and blocks again inside a minute is one
    /// interruption.
    pub fn may_notify(
        &mut self,
        at: Focus,
        state: crate::agent::State,
        now: Instant,
        floor: Duration,
    ) -> bool {
        let Focus::Ws { p, w } = at else { return false };
        let Some(ws) = self.workspace_mut(p, w) else {
            return false;
        };
        // The floor is against repeating yourself. Something different to say
        // is not repeating yourself -- and `done` pushes no further change, so
        // a suppressed one is not delayed, it is lost.
        if ws
            .notified
            .is_some_and(|(s, t)| s == state && now.duration_since(t) < floor)
        {
            return false;
        }
        ws.notified = Some((state, now));
        true
    }
}

/// What `apply` is about to set, so the clock can be reset before it is.
fn next_state(observed: Observed, seen: bool) -> crate::agent::State {
    use crate::agent::State;
    match observed {
        Observed::NoAgent => State::None,
        Observed::Blocked => State::Blocked,
        Observed::Working => State::Working,
        Observed::Waiting if seen => State::Idle,
        Observed::Waiting => State::Done,
        // A reported `done` still becomes `idle` once you have looked at it.
        // The report says the work finished; whether you have seen that is not
        // something the agent can know.
        Observed::Said(State::Done) if seen => State::Idle,
        Observed::Said(said) => said,
    }
}

fn apply(ws: &mut Workspace, observed: Observed, focused: bool) {
    use crate::agent::Source;

    // Looking at it is what "seen" means. Nothing else marks it, and reads over
    // the API must not -- otherwise a status line would clear your own
    // notifications by asking about them.
    if focused {
        ws.seen = true;
    } else if matches!(
        observed,
        Observed::Working | Observed::Said(crate::agent::State::Working)
    ) {
        // Any moment of work you are not watching leaves the result unseen.
        //
        // This was once only the *edge* into working, which missed the flow the
        // feature exists for: start something, watch it begin, then look away.
        // Focus made it seen, the edge had already passed, and it finished as
        // `idle` with nothing to say it was done.
        ws.seen = false;
    }

    if ws.state != next_state(observed, ws.seen) {
        ws.state_since = Instant::now();
        ws.turns = ws.turns.saturating_add(1);
    }
    ws.source = match observed {
        Observed::Said(_) => Source::Reported,
        Observed::Blocked => Source::Screen,
        Observed::Waiting => Source::Silence,
        _ => Source::None,
    };
    ws.state = next_state(observed, ws.seen);
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
            id: 0,
            label: String::new(),
            state,
            seen,
            notified: None,
            state_since: Instant::now(),
            intent_since: Instant::now(),
            intent: None,
            suggested: None,
            asked: None,
            turns: 0,
            touched: Instant::now(),
            reported: None,
            source: crate::agent::Source::None,
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
