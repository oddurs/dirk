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
            Section::Agents => &[("↵", "go", Action::Hint)],
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
}

impl Action {
    /// `None` when the action cannot apply right now, which is drawn dim rather
    /// than omitted — a footer that changes width as state changes is worse
    /// than one with a dead entry in it.
    fn target(self, session: &Session) -> Option<Target> {
        match self {
            Action::Hint => None,
            Action::OpenProject => Some(Target::OpenProject),
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
            Row::NewWorkspace(p) => Target::NewWorkspace(p),
            _ => return None,
        })
    }
}

/// Every row the nav would draw if it had unlimited height.
pub fn rows(session: &Session) -> Vec<Row> {
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
        }
        out.push(Row::NewWorkspace(p));
    }
    out.push(Row::Footer(Section::Spaces));

    let agents = attention(session);
    if !agents.is_empty() {
        out.push(Row::Blank);
        out.push(Row::Heading(Section::Agents, agents.len()));
        out.extend(agents.into_iter().map(|(p, w)| Row::Agent { p, w }));
        out.push(Row::Footer(Section::Agents));
    }

    out
}

/// The workspaces that are asking for something.
///
/// "Has published an intent" is the same crude signal the state glyph uses, and
/// it is what dirk can honestly tell today: a shell has no intent and a coding
/// agent publishes one continuously. 0031 makes the states real and 0021 orders
/// this by what is actually owed.
fn attention(session: &Session) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = Vec::new();
    for (p, proj) in session.projects.iter().enumerate() {
        for (w, ws) in proj.workspaces.iter().enumerate() {
            if ws.active_pane().and_then(|x| x.title()).is_some() {
                v.push((p, w));
            }
        }
    }
    v
}

/// Where the nav is looking. Distinct from focus: moving the selection does not
/// move the keyboard, and Enter is what commits it.
#[derive(Debug, Default)]
pub struct Nav {
    pub selected: usize,
    pub offset: usize,
    /// True while the nav is taking keys directly. A nav that needs the prefix
    /// before every `j` is not a nav.
    pub active: bool,
}

impl Nav {
    /// Move the selection by whole rows, skipping the scenery.
    pub fn step(&mut self, rows: &[Row], delta: isize) {
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
            self.selected = i;
        }
    }

    pub fn selection(&self, rows: &[Row]) -> Option<Row> {
        rows.get(self.selected).copied()
    }

    /// Keep the selection visible with a margin, so it never sits on the edge
    /// with no context on one side of it.
    fn scroll_to(&mut self, len: usize, height: usize) {
        const MARGIN: usize = 2;
        if len <= height {
            self.offset = 0;
            return;
        }
        let top = self.selected.saturating_sub(MARGIN);
        let bottom = (self.selected + MARGIN + 1).min(len);
        if self.offset > top {
            self.offset = top;
        } else if bottom > self.offset + height {
            self.offset = bottom - height;
        }
        self.offset = self.offset.min(len - height);
    }

    pub fn scroll_by(&mut self, delta: isize, len: usize, height: usize) {
        let max = len.saturating_sub(height);
        self.offset = (self.offset as isize + delta).clamp(0, max as isize) as usize;
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
    let all = rows(session);
    if area.width < 8 || area.height == 0 {
        return all;
    }

    nav.scroll_to(all.len(), area.height as usize);
    let inner = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    };

    for (line, index) in (nav.offset..all.len())
        .take(area.height as usize)
        .enumerate()
    {
        let y = area.y + line as u16;
        let full = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };
        let row = all[index];

        // A selected row is marked whether or not the nav holds the keyboard,
        // but only brightly while it does — otherwise two things on screen
        // claim to be "where you are".
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
                y,
                session,
                selected: nav.selected == index && row.selectable(),
                active: nav.active,
            },
        );
        if let Some(t) = row.target() {
            hits.push(full, t);
        }
    }

    // A list with more below it should say so, rather than simply ending.
    if all.len() > area.height as usize {
        let more = all.len() - nav.offset - area.height as usize;
        if more > 0 {
            let y = area.y + area.height - 1;
            let text = format!("{more} more");
            let x = area.right().saturating_sub(text.chars().count() as u16 + 1);
            write_str(buf, x, y, &text, THEME.faint(), area.width);
        }
    }

    all
}

/// What one row needs to know about everything outside it. Eight parameters
/// was a struct that had not been written down yet.
struct Ctx<'a> {
    inner: Rect,
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
            let n = count.to_string();
            let x = inner.right().saturating_sub(n.chars().count() as u16);
            write_str(buf, x, y, &n, THEME.faint(), w);
        }

        Row::Footer(section) => {
            let mut x = inner.x + 2;
            for (i, (key, label, action)) in section.actions().iter().enumerate() {
                if i > 0 {
                    x += write_str(buf, x, y, "  ·  ", THEME.faint(), w);
                }
                let start = x;
                let live = action.target(session);
                let (kstyle, lstyle) = match live {
                    Some(_) => (THEME.key(), THEME.dim()),
                    None => (THEME.faint(), THEME.faint()),
                };
                x += write_str(buf, x, y, key, kstyle, w);
                x += write_str(buf, x, y, " ", THEME.faint(), w);
                x += write_str(buf, x, y, label, lstyle, w);
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
            space_row(buf, cx, ws, p, wi, None, false);
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

        Row::NewWorkspace(_) => {
            let style = if selected && active {
                base
            } else {
                THEME.faint()
            };
            write_str(buf, inner.x + 2, y, "+ workspace", style, w);
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
    x += write_str(buf, x, y, "  ", THEME.rule_strong(), w);
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

/// A workspace's state is its active pane's, since a workspace with one pane is
/// the common case and one with two has no single answer anyway.
fn state_of(ws: &Workspace) -> &'static str {
    match ws.active_pane() {
        None => "unknown",
        Some(p) if p.dead => "idle",
        // Without agent detection dirk cannot tell working from blocked; a pane
        // that has published an intent is doing something, one that has not is
        // a shell. Real states arrive with 0030 and 0031.
        Some(p) if p.title().is_some() => "working",
        Some(_) => "idle",
    }
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
    fn scrolling_keeps_the_selection_off_the_edges() {
        let rows: Vec<Row> = (0..40)
            .map(|w| Row::Workspace { p: 0, w, n: w + 1 })
            .collect();
        let mut nav = Nav::default();
        let height = 10;

        nav.selected = 0;
        nav.scroll_to(rows.len(), height);
        assert_eq!(
            nav.offset, 0,
            "the top does not scroll to make room above it"
        );

        nav.selected = 20;
        nav.scroll_to(rows.len(), height);
        assert!(
            nav.offset <= 20 && 20 < nav.offset + height,
            "selection visible"
        );
        assert!(20 - nav.offset >= 2, "with room above");
        assert!(nav.offset + height - 20 > 2, "and below");

        nav.selected = 39;
        nav.scroll_to(rows.len(), height);
        assert_eq!(
            nav.offset, 30,
            "the bottom stops rather than scrolling past"
        );
    }

    #[test]
    fn a_list_shorter_than_the_pane_never_scrolls() {
        let rows = sample();
        let mut nav = Nav {
            selected: rows.len() - 1,
            ..Nav::default()
        };
        nav.scroll_to(rows.len(), 40);
        assert_eq!(nav.offset, 0);
    }

    #[test]
    fn the_wheel_cannot_scroll_past_either_end() {
        let rows: Vec<Row> = (0..20)
            .map(|w| Row::Workspace { p: 0, w, n: w + 1 })
            .collect();
        let mut nav = Nav::default();
        nav.scroll_by(-5, rows.len(), 10);
        assert_eq!(nav.offset, 0);
        nav.scroll_by(500, rows.len(), 10);
        assert_eq!(nav.offset, 10, "the last row stays on screen");
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
}
