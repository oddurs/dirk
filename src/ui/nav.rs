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

//! The nav: three lists down the left of everything.
//!
//! ```text
//! layouts                3
//!  1 • Overview
//!    ↵ open
//!
//! spaces                10
//!  ▾ dirk
//!    * 1 Building the mux core     2m
//!        main
//!    · 2 Reading the vt100 grid ⑂  1d
//!        feat/packaging-manifests
//!    + workspace
//!    n new  ·  o project
//!
//! agents                 1
//!    * Building the mux core
//! ```
//!
//! Three lists of different things, and the order they appear in is the
//! argument: **layouts** are places you go, **spaces** are where work lives, and
//! **agents** are what is asking for you. Attention flows down the column.
//!
//! The same workspace appears under spaces and under agents. That is not
//! duplication — spaces answers "what is open, and where" and agents answers
//! "what needs me", and they are sorted differently for exactly that reason.
//!
//! Everything is built as a flat list of rows first and rendered as a window
//! onto it. Scrolling is then an offset, selection an index, and hit testing the
//! same lookup the renderer already does; done as one pass with a moving cursor,
//! each of those is awkward on its own.

use crate::hit::{HitMap, Target};
use crate::mux::session::since;
use crate::mux::{Focus, Session, Workspace};
use crate::theme::THEME;
use crate::ui::{elide, fill, heading, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Layouts,
    Spaces,
    Agents,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Section::Layouts => "layouts",
            Section::Spaces => "spaces",
            Section::Agents => "agents",
        }
    }

    /// What the list can do, shown under it. An action a list supports should
    /// be visible in the list rather than remembered.
    fn actions(self) -> &'static [(&'static str, &'static str, Action)] {
        match self {
            Section::Layouts => &[("↵", "open", Action::Hint)],
            Section::Spaces => &[
                ("n", "new", Action::NewWorkspace),
                ("o", "project", Action::OpenProject),
            ],
            Section::Agents => &[("↵", "go", Action::Hint), ("s", "sort", Action::Sort)],
        }
    }
}

/// What a footer entry does when clicked. `Hint` is a reminder of a key that
/// already works, not a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Hint,
    NewWorkspace,
    OpenProject,
    Sort,
}

impl Action {
    /// `None` when the action cannot apply right now, which is drawn dim rather
    /// than omitted — a footer that changes width as state changes is worse
    /// than one with a dead entry in it.
    fn target(self, session: &Session) -> Option<Target> {
        match self {
            Action::Hint => None,
            Action::OpenProject => Some(Target::OpenProject),
            Action::Sort => Some(Target::SortAgents),
            Action::NewWorkspace => match session.focus {
                Focus::Ws { p, .. } => Some(Target::NewWorkspace(p)),
                Focus::Layout(_) => None,
            },
        }
    }
}

/// One line of the nav.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Heading(Section, usize),
    Layout(usize),
    Project(usize),
    /// The identity line of a workspace: state, number, name, age.
    Workspace {
        p: usize,
        w: usize,
        n: usize,
    },
    /// Its second line: which checkout this is. Scenery — the selection lands
    /// on the identity line, and clicking either goes to the same place.
    Branch {
        p: usize,
        w: usize,
    },
    /// One pane of an expanded workspace.
    Pane {
        p: usize,
        w: usize,
        index: usize,
    },
    /// The same workspace, in the agents list. A separate variant so that
    /// selecting one does not also look selected in the other.
    Agent {
        p: usize,
        w: usize,
    },
    NewWorkspace(usize),
    Footer(Section),
    Blank,
}

impl Row {
    /// Rows the selection can land on. Headings, footers and blanks are
    /// scenery: stopping on them would make `j` feel broken.
    pub fn selectable(self) -> bool {
        !matches!(
            self,
            Row::Heading(..) | Row::Footer(_) | Row::Blank | Row::Branch { .. }
        )
    }

    /// What activating this row means. Keyboard and pointer produce the same
    /// value and go through the same handler, so they cannot drift apart.
    pub fn target(self) -> Option<Target> {
        Some(match self {
            Row::Layout(i) => Target::Layout(i),
            Row::Project(i) => Target::ProjectFold(i),
            Row::Workspace { p, w, .. } | Row::Agent { p, w } | Row::Branch { p, w } => {
                Target::Workspace { p, w }
            }
            Row::Pane { p, w, index } => Target::NavPane { p, w, index },
            Row::NewWorkspace(p) => Target::NewWorkspace(p),
            _ => return None,
        })
    }
}

/// Every row the nav would draw if it had unlimited height.
pub fn rows(session: &Session, sort: Sort) -> Vec<Row> {
    let mut out = Vec::new();

    if !session.layouts.is_empty() {
        out.push(Row::Heading(Section::Layouts, session.layouts.len()));
        out.extend((0..session.layouts.len()).map(Row::Layout));
        out.push(Row::Footer(Section::Layouts));
        out.push(Row::Blank);
    }

    let spaces: usize = session.projects.iter().map(|p| p.workspaces.len()).sum();
    out.push(Row::Heading(Section::Spaces, spaces));
    // Numbers run across the whole list rather than per project, because they
    // are jump keys and the rail numbers the same way.
    let mut n = 0;
    for (p, proj) in session.projects.iter().enumerate() {
        out.push(Row::Project(p));
        if !proj.expanded {
            n += proj.workspaces.len();
            continue;
        }
        for w in 0..proj.workspaces.len() {
            n += 1;
            out.push(Row::Workspace { p, w, n });
            // Only when there is something to say. A row with nothing for its
            // second line draws one line rather than a blank one.
            if proj.repo.as_ref().is_some_and(|r| !r.branch.is_empty()) {
                out.push(Row::Branch { p, w });
            }
            // A workspace with one pane draws no subtree: there is nothing the
            // row above does not already say.
            let ws = &proj.workspaces[w];
            if ws.expanded && ws.panes.len() > 1 {
                out.extend((0..ws.tree.leaves().len()).map(|index| Row::Pane { p, w, index }));
            }
        }
        out.push(Row::NewWorkspace(p));
    }
    out.push(Row::Footer(Section::Spaces));

    let agents = attention(session, sort);
    if !agents.is_empty() {
        out.push(Row::Blank);
        out.push(Row::Heading(Section::Agents, agents.len()));
        out.extend(agents.into_iter().map(|(p, w)| Row::Agent { p, w }));
        out.push(Row::Footer(Section::Agents));
    }

    out
}

/// How the agents list is ordered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    /// What is owed: blocked, then finished-and-unseen, then working, then
    /// idle. Oldest first inside each, because the thing that has been waiting
    /// longest is the thing that has been waiting longest.
    #[default]
    Attention,
    /// Most recently active first, for following what is happening rather than
    /// clearing what is owed.
    Recent,
}

impl Sort {
    fn label(self) -> &'static str {
        match self {
            Sort::Attention => "attention",
            Sort::Recent => "recent",
        }
    }

    fn next(self) -> Self {
        match self {
            Sort::Attention => Sort::Recent,
            Sort::Recent => Sort::Attention,
        }
    }
}

/// Lower is more urgent. `blocked` is first because it is the only state
/// waiting on a human; `done` next because it is finished work nobody has
/// looked at.
fn rank(state: &str) -> u8 {
    match state {
        "blocked" => 0,
        "done" => 1,
        "working" => 2,
        "idle" => 3,
        _ => 4,
    }
}

/// The workspaces that are asking for something, in the order they are asking.
///
/// Spaces are listed in a stable order so the number beside one is a jump key
/// you can learn. That is right for navigation and exactly wrong for triage:
/// the agent that has been blocked for ten minutes is wherever its workspace
/// happens to sit. This is the same set, ordered by what is owed.
///
/// Membership is "holds a recognised agent", which dirk now reads from the
/// pane's foreground process group. It used to be "has published a title", and
/// that was wrong in a way worth naming: shells set titles too, so every plain
/// shell was listed here as something wanting attention.
fn attention(session: &Session, sort: Sort) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = Vec::new();
    for (p, proj) in session.projects.iter().enumerate() {
        for (w, ws) in proj.workspaces.iter().enumerate() {
            // The same field the glyph reads. Asking the occupant here and
            // the state there was two answers to one question, and they
            // disagreed on screen.
            if ws.state != crate::agent::State::None {
                v.push((p, w));
            }
        }
    }
    v.sort_by_key(|&(p, w)| {
        let ws = &session.projects[p].workspaces[w];
        let age = ws.touched.elapsed().as_secs();
        match sort {
            // Oldest first within a rank, so `age` sorts descending.
            Sort::Attention => (rank(state_of(ws)), u64::MAX - age),
            Sort::Recent => (0, age),
        }
    });
    v
}

/// A section as it was last drawn.
#[derive(Debug, Clone, Copy, Default)]
pub struct Band {
    index: usize,
    len: usize,
    y: u16,
    height: u16,
}

/// Keep a section's selection visible inside its own band, with a margin.
fn scroll_within(offset: &mut usize, at: usize, len: usize, height: usize) {
    const MARGIN: usize = 1;
    if len <= height {
        *offset = 0;
        return;
    }
    let top = at.saturating_sub(MARGIN);
    let bottom = (at + MARGIN + 1).min(len);
    if *offset > top {
        *offset = top;
    } else if bottom > *offset + height {
        *offset = bottom - height;
    }
    *offset = (*offset).min(len - height);
}

/// Split the flat row list into one range per section.
///
/// The list is flat because selection, hit testing and targets all want one
/// index space. Height, though, is allocated per section — so this is the one
/// place that has to know where the seams are.
pub fn sections(rows: &[Row]) -> Vec<(Section, std::ops::Range<usize>)> {
    let mut out: Vec<(Section, std::ops::Range<usize>)> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if let Row::Heading(section, _) = row {
            if let Some(last) = out.last_mut() {
                last.1.end = i;
            }
            out.push((*section, i..rows.len()));
        }
    }
    out
}

/// Give each section a height.
///
/// A section that fits gets exactly what it needs; only sections asking for
/// more than their share are shrunk, and the space a short section did not want
/// goes to the ones that did. Without this the nav is one list with one offset
/// and whatever is last simply falls off the bottom — which is the agents
/// section, the one that exists to be noticed.
pub fn allocate(wants: &[usize], height: usize) -> Vec<usize> {
    let total: usize = wants.iter().sum();
    if total <= height || wants.is_empty() {
        return wants.to_vec();
    }

    // Everyone is offered an equal share. Whoever wants less takes only what
    // they want, and what they leave is offered round again.
    let mut given = vec![0usize; wants.len()];
    let mut left = height;
    let mut open: Vec<usize> = (0..wants.len()).collect();

    while !open.is_empty() {
        let share = left / open.len();
        // Nothing left to divide: the remainder goes to the first claimants,
        // one line each, rather than to nobody.
        if share == 0 {
            for (n, &i) in open.iter().enumerate() {
                given[i] += usize::from(n < left);
            }
            break;
        }
        let (small, big): (Vec<usize>, Vec<usize>) =
            open.iter().partition(|&&i| wants[i] - given[i] <= share);
        if small.is_empty() {
            for &i in &open {
                given[i] += share;
            }
            // Whatever rounding left over goes to the first section, which is
            // the one nearest the top of the screen.
            let used = share * open.len();
            given[open[0]] += left - used;
            break;
        }
        for &i in &small {
            left -= wants[i] - given[i];
            given[i] = wants[i];
        }
        open = big;
    }
    given
}

/// Where the nav is looking. Distinct from focus: moving the selection does not
/// move the keyboard, and Enter is what commits it.
#[derive(Debug, Default)]
pub struct Nav {
    pub selected: usize,
    pub sort: Sort,
    /// One scroll offset per section, keyed by position in `sections()`. A
    /// section that fits is never scrolled at all.
    offsets: Vec<usize>,
    /// Where each section ended up on screen, so the wheel can find the one
    /// under the pointer.
    bands: Vec<Band>,
    /// Whether the view should chase the selection on the next frame.
    ///
    /// Set when the selection moves, cleared once the scroll has happened.
    /// Without it, scrolling to the selection every frame silently undoes the
    /// wheel between one redraw and the next, and the wheel does nothing at
    /// all.
    follow: bool,
    /// True while the nav is taking keys directly. A nav that needs the prefix
    /// before every `j` is not a nav.
    pub active: bool,
}

impl Nav {
    /// Move the selection by whole rows, skipping the scenery.
    pub fn step(&mut self, rows: &[Row], delta: isize) {
        self.follow = true;
        let pick: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selectable())
            .map(|(i, _)| i)
            .collect();
        if pick.is_empty() {
            return;
        }
        let at = pick
            .iter()
            .position(|&i| i == self.selected)
            .map(|x| x as isize);
        let next = match at {
            Some(x) => (x + delta).rem_euclid(pick.len() as isize) as usize,
            // Coming from nowhere, `j` should land on the first row rather than
            // the second.
            None if delta >= 0 => 0,
            None => pick.len() - 1,
        };
        self.selected = pick[next];
    }

    /// Put the selection on the focused workspace, so entering the nav starts
    /// where the eye already is.
    pub fn sync(&mut self, rows: &[Row], focus: Focus) {
        let at = |r: &Row| match (focus, r) {
            (Focus::Layout(i), Row::Layout(j)) => i == *j,
            (Focus::Ws { p, w }, Row::Workspace { p: q, w: x, .. }) => p == *q && w == *x,
            _ => false,
        };
        if let Some(i) = rows.iter().position(at) {
            if i != self.selected {
                self.follow = true;
            }
            self.selected = i;
        }
    }

    pub fn selection(&self, rows: &[Row]) -> Option<Row> {
        rows.get(self.selected).copied()
    }

    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
    }

    /// Scroll the section under the pointer, without moving the selection.
    pub fn scroll_by(&mut self, row: u16, delta: isize) {
        let Some(band) = self
            .bands
            .iter()
            .find(|b| row >= b.y && row < b.y + b.height)
        else {
            return;
        };
        let (i, len, height) = (band.index, band.len, band.height as usize);
        let max = len.saturating_sub(height);
        if let Some(off) = self.offsets.get_mut(i) {
            *off = (*off as isize + delta).clamp(0, max as isize) as usize;
        }
        // The view is now somewhere the selection did not ask for, and the next
        // frame must not drag it back.
        self.follow = false;
    }
}

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    session: &Session,
    nav: &mut Nav,
    hits: &mut HitMap,
) -> Vec<Row> {
    fill(buf, area, THEME.panel());
    let all = rows(session, nav.sort);
    if area.width < 8 || area.height == 0 {
        return all;
    }

    // The highlight marks where you are. While the nav is not driving, that is
    // the focused row -- focus moves by clicking, by prefix keys and by opening
    // a project, none of which go through the nav.
    if !nav.active {
        nav.sync(&all, session.focus);
    }

    let bands = sections(&all);
    let wants: Vec<usize> = bands.iter().map(|(_, r)| r.len()).collect();
    let heights = allocate(&wants, area.height as usize);
    nav.offsets.resize(bands.len(), 0);
    nav.bands.clear();

    let follow = std::mem::take(&mut nav.follow);
    let inner = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    };
    let mut y = area.y;

    for (b, ((_, range), height)) in bands.iter().zip(heights.iter().copied()).enumerate() {
        if height == 0 {
            continue;
        }
        let len = range.len();
        let offset = &mut nav.offsets[b];
        *offset = (*offset).min(len.saturating_sub(height));
        if follow && range.contains(&nav.selected) {
            scroll_within(offset, nav.selected - range.start, len, height);
        }
        let offset = *offset;
        nav.bands.push(Band {
            index: b,
            len,
            y,
            height: height as u16,
        });

        for line in 0..height {
            let index = range.start + offset + line;
            if index >= range.end {
                break;
            }
            let row = all[index];
            let ry = y + line as u16;
            let full = Rect {
                x: area.x,
                y: ry,
                width: area.width,
                height: 1,
            };

            // A selected row is marked whether or not the nav holds the
            // keyboard, but only brightly while it does -- otherwise two things
            // on screen claim to be "where you are".
            if nav.selected == index && row.selectable() {
                fill(
                    buf,
                    full,
                    if nav.active {
                        THEME.selected()
                    } else {
                        THEME.active_row()
                    },
                );
            }
            draw(
                buf,
                hits,
                row,
                &Ctx {
                    inner,
                    sort: nav.sort,
                    y: ry,
                    session,
                    selected: nav.selected == index && row.selectable(),
                    active: nav.active,
                },
            );
            if let Some(t) = row.target() {
                hits.push(full, t);
            }
        }

        // A section with more below it says so on its own last line, rather
        // than simply ending.
        let hidden = len.saturating_sub(offset + height);
        if hidden > 0 {
            let text = format!("{hidden} more");
            let x = inner.right().saturating_sub(text.chars().count() as u16);
            write_str(
                buf,
                x,
                y + height as u16 - 1,
                &text,
                THEME.faint(),
                inner.width,
            );
        }

        y += height as u16;
    }

    all
}

/// What one row needs to know about everything outside it. Eight parameters
/// was a struct that had not been written down yet.
struct Ctx<'a> {
    inner: Rect,
    sort: Sort,
    y: u16,
    session: &'a Session,
    /// This row is the selection.
    selected: bool,
    /// The nav holds the keyboard.
    active: bool,
}

fn draw(buf: &mut Buffer, hits: &mut HitMap, row: Row, cx: &Ctx) {
    let Ctx {
        inner,
        y,
        session,
        selected,
        active,
        ..
    } = *cx;
    let w = inner.width;
    let base = if selected && active {
        THEME.selected()
    } else {
        THEME.text()
    };

    match row {
        Row::Blank => {}

        Row::Heading(section, count) => {
            heading(
                buf,
                Rect {
                    x: inner.x,
                    y,
                    width: w,
                    height: 1,
                },
                section.title(),
                THEME.title(),
            );
            // The agents list is the only one whose order is a choice, so it
            // is the only one that has to say what the choice currently is.
            let right = match section {
                Section::Agents => cx.sort.label().to_string(),
                _ => count.to_string(),
            };
            let x = inner.right().saturating_sub(right.chars().count() as u16);
            write_str(buf, x, y, &right, THEME.faint(), w);
        }

        Row::Footer(section) => {
            let mut x = inner.x + 2;
            for (i, (key, label, action)) in section.actions().iter().enumerate() {
                if i > 0 {
                    x += write_str(
                        buf,
                        x,
                        y,
                        "  ·  ",
                        THEME.faint(),
                        inner.right().saturating_sub(x),
                    );
                }
                let start = x;
                let live = action.target(session);
                let (kstyle, lstyle) = match live {
                    Some(_) => (THEME.key(), THEME.dim()),
                    None => (THEME.faint(), THEME.faint()),
                };
                // The budget is what is left from here, not the whole width.
                // Passing the width lets a long footer paint past the nav's
                // right edge into the panes, where only the next blit hides it.
                let left = |x: u16| inner.right().saturating_sub(x);
                x += write_str(buf, x, y, key, kstyle, left(x));
                x += write_str(buf, x, y, " ", THEME.faint(), left(x));
                x += write_str(buf, x, y, label, lstyle, left(x));
                if let Some(t) = live {
                    hits.push(
                        Rect {
                            x: start,
                            y,
                            width: x - start,
                            height: 1,
                        },
                        t,
                    );
                }
            }
        }

        Row::Layout(i) => {
            let Some(layout) = session.layouts.get(i) else {
                return;
            };
            let open = layout.ws.is_some();
            let focused = session.focus == Focus::Layout(i);
            let style = if selected && active {
                base
            } else if open {
                THEME.text()
            } else {
                THEME.dim()
            };

            let mut x = inner.x;
            if let Some(k) = layout.def.key {
                x += write_str(buf, x, y, &format!("{k} "), THEME.key(), w);
            }
            // A dot for a layout that is open, so "running" and "not yet built"
            // are distinguishable without a second column.
            x += write_str(buf, x, y, if open { "• " } else { "  " }, THEME.ok(), w);
            let left = w.saturating_sub(x - inner.x) as usize;
            let style = if focused {
                style.patch(THEME.project())
            } else {
                style
            };
            write_str(buf, x, y, &elide(&layout.def.name, left), style, w);
        }

        Row::Project(p) => {
            let Some(proj) = session.projects.get(p) else {
                return;
            };
            let mut x = inner.x;
            x += write_str(
                buf,
                x,
                y,
                if proj.expanded { "▾ " } else { "▸ " },
                THEME.rule_strong(),
                w,
            );
            let left = w.saturating_sub(x - inner.x).saturating_sub(3) as usize;
            x += write_str(buf, x, y, &elide(&proj.name, left), THEME.project(), w);
            if !proj.expanded && proj.workspaces.len() > 1 {
                write_str(
                    buf,
                    x + 1,
                    y,
                    &proj.workspaces.len().to_string(),
                    THEME.faint(),
                    w,
                );
            }
        }

        Row::Workspace { p, w: wi, n } => {
            let Some(ws) = session.workspace(p, wi) else {
                return;
            };
            let worktree = session
                .projects
                .get(p)
                .is_some_and(|x| x.repo.as_ref().is_some_and(|r| r.worktree));
            space_row(buf, cx, ws, p, wi, Some(n), worktree);
        }

        Row::Agent { p, w: wi } => {
            let Some(ws) = session.workspace(p, wi) else {
                return;
            };
            // Worktrees are marked here too. This is the list where telling
            // them apart matters most: several agents in parallel means several
            // checkouts of one repository, all wearing its name.
            let worktree = session
                .projects
                .get(p)
                .is_some_and(|x| x.repo.as_ref().is_some_and(|r| r.worktree));
            space_row(buf, cx, ws, p, wi, None, worktree);
        }

        Row::Branch { p, w: _ } => {
            let Some(repo) = session.projects.get(p).and_then(|x| x.repo.as_ref()) else {
                return;
            };
            let style = if selected && active {
                base
            } else {
                THEME.branch()
            };
            let left = w.saturating_sub(6) as usize;
            write_str(buf, inner.x + 6, y, &elide(&repo.branch, left), style, w);
        }

        Row::Pane { p, w: wi, index } => {
            let Some(ws) = session.workspace(p, wi) else {
                return;
            };
            let Some(&id) = ws.tree.leaves().get(index) else {
                return;
            };
            let Some(pane) = ws.pane(id) else {
                return;
            };
            let last = index + 1 == ws.tree.leaves().len();
            let style = if selected && active {
                base
            } else {
                THEME.faint()
            };

            let mut x = inner.x + 4;
            x += write_str(
                buf,
                x,
                y,
                if last { "└ " } else { "├ " },
                THEME.rule_strong(),
                w,
            );
            // A pane's own label if a layout gave it one, otherwise whatever
            // the program inside is calling itself.
            let name = pane
                .label
                .clone()
                .or_else(|| pane.title())
                .unwrap_or_else(|| {
                    // Failing a label or a title, say what is running: that is more
                    // use than the directory, which the row above already implies.
                    match &pane.occupant {
                        crate::agent::Occupant::Agent(k) => k.name.to_string(),
                        crate::agent::Occupant::Program(p) => p.clone(),
                        // "free" rather than "shell": what matters about a
                        // prompt is that an agent could be started in it.
                        o if o.available() => "free".into(),
                        // Nothing known yet -- the first tick after a split, or
                        // a platform with no foreground group to read. The
                        // directory is a poor name and an empty row is worse.
                        _ => pane
                            .cwd
                            .file_name()
                            .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
                    }
                });
            let left = w.saturating_sub(x - inner.x) as usize;
            write_str(buf, x, y, &elide(&name, left), style, w);
        }

        Row::NewWorkspace(_) => {
            let style = if selected && active {
                base
            } else {
                THEME.faint()
            };
            write_str(
                buf,
                inner.x + 2,
                y,
                "+ workspace",
                style,
                w.saturating_sub(2),
            );
        }
    }
}

/// One workspace's identity line: state, number, name, and how long since it
/// last said anything.
///
/// The age is right-aligned and reserved before the name is written, so a long
/// intent elides rather than colliding with it.
fn space_row(
    buf: &mut Buffer,
    cx: &Ctx,
    ws: &Workspace,
    p: usize,
    wi: usize,
    number: Option<usize>,
    worktree: bool,
) {
    let Ctx {
        inner,
        y,
        session,
        selected,
        active,
        ..
    } = *cx;
    let w = inner.width;
    let focused = session.focus == Focus::Ws { p, w: wi };
    let (glyph, gstyle) = THEME.agent_state(state_of(ws));

    let age = since(ws.touched.elapsed());
    let age_w = age.chars().count() as u16;
    write_str(
        buf,
        inner.right().saturating_sub(age_w),
        y,
        &age,
        THEME.faint(),
        age_w,
    );

    let mut x = inner.x;
    let arrow = match (ws.panes.len() > 1, ws.expanded) {
        (false, _) => "  ",
        (true, false) => "▸ ",
        (true, true) => "▾ ",
    };
    x += write_str(buf, x, y, arrow, THEME.rule_strong(), w);
    x += write_str(buf, x, y, glyph, gstyle, w);
    x += write_str(buf, x, y, " ", THEME.text(), w);
    if let Some(n) = number {
        x += write_str(buf, x, y, &format!("{n} "), THEME.number(), w);
    }

    let style = match (focused, selected && active) {
        (true, _) => THEME.text(),
        (false, true) => THEME.selected(),
        (false, false) => THEME.intent(),
    };
    // The name gets what is left after the age, plus a space so the two never
    // touch, plus the worktree mark when there is one.
    let mark = if worktree { 2 } else { 0 };
    let left = w
        .saturating_sub(x - inner.x)
        .saturating_sub(age_w + 1 + mark) as usize;
    x += write_str(buf, x, y, &elide(&ws.label, left), style, w);
    if worktree {
        write_str(buf, x + 1, y, "⑂", THEME.worktree(), w);
    }
}

/// A workspace's state, as the word the theme and the nav both speak.
///
/// Worked out in `session::update_states`, which is where it belongs: it
/// depends on whether *you* have looked at the workspace, and drawing should
/// not be the thing deciding that.
fn state_of(ws: &Workspace) -> &'static str {
    ws.state.glyph_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nav shaped like a real one: a layout, a project with two workspaces,
    /// and the scenery between them.
    fn sample() -> Vec<Row> {
        vec![
            Row::Heading(Section::Layouts, 1),
            Row::Layout(0),
            Row::Footer(Section::Layouts),
            Row::Blank,
            Row::Heading(Section::Spaces, 2),
            Row::Project(0),
            Row::Workspace { p: 0, w: 0, n: 1 },
            Row::Branch { p: 0, w: 0 },
            Row::Workspace { p: 0, w: 1, n: 2 },
            Row::NewWorkspace(0),
            Row::Footer(Section::Spaces),
        ]
    }

    #[test]
    fn the_selection_skips_the_scenery() {
        let rows = sample();
        let mut nav = Nav::default();

        // From nothing, `j` lands on the first selectable row rather than the
        // second.
        nav.step(&rows, 1);
        assert_eq!(nav.selection(&rows), Some(Row::Layout(0)));

        nav.step(&rows, 1);
        assert_eq!(
            nav.selection(&rows),
            Some(Row::Project(0)),
            "headings are not stops"
        );
        nav.step(&rows, 1);
        assert_eq!(
            nav.selection(&rows),
            Some(Row::Workspace { p: 0, w: 0, n: 1 })
        );
    }

    #[test]
    fn the_selection_wraps_both_ways() {
        let rows = sample();
        let mut nav = Nav::default();
        nav.step(&rows, 1);
        nav.step(&rows, -1);
        assert_eq!(
            nav.selection(&rows),
            Some(Row::NewWorkspace(0)),
            "up from the top wraps"
        );
        nav.step(&rows, 1);
        assert_eq!(
            nav.selection(&rows),
            Some(Row::Layout(0)),
            "and back down again"
        );
    }

    #[test]
    fn stepping_an_empty_nav_does_nothing() {
        let mut nav = Nav::default();
        nav.step(&[], 1);
        assert_eq!(nav.selected, 0);
    }

    #[test]
    fn entering_the_nav_starts_where_the_eye_is() {
        let rows = sample();
        let mut nav = Nav::default();
        nav.sync(&rows, Focus::Ws { p: 0, w: 1 });
        assert_eq!(
            nav.selection(&rows),
            Some(Row::Workspace { p: 0, w: 1, n: 2 })
        );
        nav.sync(&rows, Focus::Layout(0));
        assert_eq!(nav.selection(&rows), Some(Row::Layout(0)));
    }

    #[test]
    fn a_focus_with_no_row_leaves_the_selection_alone() {
        let rows = sample();
        let mut nav = Nav::default();
        nav.sync(&rows, Focus::Ws { p: 0, w: 0 });
        let before = nav.selected;
        nav.sync(&rows, Focus::Ws { p: 9, w: 9 });
        assert_eq!(nav.selected, before);
    }

    #[test]
    fn every_selectable_row_knows_what_activating_it_means() {
        for row in sample() {
            if row.selectable() {
                assert!(
                    row.target().is_some(),
                    "{row:?} can be selected but does nothing"
                );
            }
        }
    }

    #[test]
    fn a_workspaces_second_line_goes_where_its_first_line_goes() {
        // Clicking the branch under a name is still clicking that workspace,
        // even though the selection never lands on it.
        let branch = Row::Branch { p: 0, w: 0 };
        assert!(!branch.selectable());
        assert_eq!(
            branch.target(),
            Row::Workspace { p: 0, w: 0, n: 1 }.target()
        );
    }

    #[test]
    fn the_selection_steps_over_a_second_line() {
        let rows = sample();
        let mut nav = Nav::default();
        nav.sync(&rows, Focus::Ws { p: 0, w: 0 });
        nav.step(&rows, 1);
        assert_eq!(
            nav.selection(&rows),
            Some(Row::Workspace { p: 0, w: 1, n: 2 }),
            "`j` should reach the next workspace, not its branch line"
        );
    }

    #[test]
    fn attention_puts_the_blocked_agent_first() {
        // blocked is the only state waiting on a human, and done is finished
        // work nobody has looked at. Both outrank anything still running.
        let mut states = ["idle", "working", "done", "blocked", "unknown"];
        states.sort_by_key(|s| rank(s));
        assert_eq!(states, ["blocked", "done", "working", "idle", "unknown"]);
    }

    #[test]
    fn the_sort_toggle_comes_back_to_where_it_started() {
        let s = Sort::default();
        assert_eq!(s, Sort::Attention, "triage is the default, not chronology");
        assert_eq!(s.next().next(), s);
        assert_ne!(s.label(), s.next().label());
    }

    #[test]
    fn a_pane_row_goes_to_that_pane_and_not_just_its_workspace() {
        let pane = Row::Pane {
            p: 1,
            w: 2,
            index: 3,
        };
        assert!(pane.selectable());
        assert_eq!(
            pane.target(),
            Some(Target::NavPane {
                p: 1,
                w: 2,
                index: 3
            })
        );
        // And it is a different destination from the workspace row above it.
        assert_ne!(pane.target(), Row::Workspace { p: 1, w: 2, n: 1 }.target());
    }

    #[test]
    fn sections_that_fit_all_get_what_they_asked_for() {
        assert_eq!(allocate(&[3, 5, 2], 20), vec![3, 5, 2]);
        assert_eq!(allocate(&[3, 5, 2], 10), vec![3, 5, 2]);
    }

    #[test]
    fn only_the_greedy_sections_are_shrunk() {
        // Layouts wants 3, spaces wants 40, agents wants 4, and there are 20
        // lines. The two small sections should get exactly what they need; the
        // large one absorbs the shortfall on its own.
        let given = allocate(&[3, 40, 4], 20);
        assert_eq!(given[0], 3, "a section that fits should not be shrunk");
        assert_eq!(given[2], 4, "nor should the one after the greedy one");
        assert_eq!(given[1], 13);
        assert_eq!(given.iter().sum::<usize>(), 20);
    }

    #[test]
    fn the_agents_section_is_never_the_one_silently_cut() {
        // The failure this exists to prevent: one flat list meant a long spaces
        // section pushed agents off the bottom entirely.
        let given = allocate(&[3, 100, 4], 20);
        assert!(given[2] > 0, "agents got no height at all: {given:?}");
        assert_eq!(given.iter().sum::<usize>(), 20);
    }

    #[test]
    fn every_line_is_handed_out_even_when_there_are_barely_any() {
        for height in 0..12 {
            let given = allocate(&[5, 5, 5], height);
            assert_eq!(
                given.iter().sum::<usize>(),
                height,
                "at height {height}: {given:?}"
            );
        }
    }

    #[test]
    fn sections_are_found_by_their_headings() {
        let rows = sample();
        let found = sections(&rows);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, Section::Layouts);
        assert_eq!(found[1].0, Section::Spaces);
        // Contiguous, and covering everything from the first heading on.
        assert_eq!(found[0].1.end, found[1].1.start);
        assert_eq!(found[1].1.end, rows.len());
    }

    #[test]
    fn a_section_that_fits_is_never_scrolled() {
        let mut offset = 3;
        scroll_within(&mut offset, 2, 4, 10);
        assert_eq!(offset, 0);
    }

    #[test]
    fn a_section_scrolls_to_keep_its_own_selection_visible() {
        let mut offset = 0;
        scroll_within(&mut offset, 18, 40, 6);
        assert!(
            offset <= 18 && 18 < offset + 6,
            "selection outside the band"
        );
        // And stops rather than scrolling past the end.
        scroll_within(&mut offset, 39, 40, 6);
        assert_eq!(offset, 34);
    }
}
