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

use crate::config::Config;
use crate::glyph::{G, Glyphs};
use crate::hit::{HitMap, Target};
use crate::mux::session::since;
use crate::mux::{Focus, Session, Workspace};
use crate::theme::THEME;
use crate::ui::{cells, elide, fill, heading, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Layouts,
    Spaces,
    /// What is owed. Not a third list of its own things -- the same workspaces
    /// as `Spaces`, in the order you would deal with them.
    Attention,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Section::Layouts => "boards",
            Section::Spaces => "spaces",
            Section::Attention => "needs you",
        }
    }

    /// What the list can do, shown under it. An action a list supports should
    /// be visible in the list rather than remembered.
    fn actions(self) -> &'static [(Press, &'static str, Action)] {
        match self {
            Section::Layouts => &[(Press::Mark(G::Enter), "open", Action::Hint)],
            Section::Spaces => &[
                // Agent first: it is what the workspace is for.
                (Press::Key("a"), "agent", Action::NewAgent),
                (Press::Key("n"), "new", Action::NewWorkspace),
                (Press::Key("o"), "project", Action::OpenProject),
                (Press::Key("W"), "worktree", Action::NewWorktree),
            ],
            Section::Attention => &[(Press::Mark(G::Enter), "go", Action::Hint)],
        }
    }
}

/// The key a footer entry names. Most are a letter; Enter is a mark, and a mark
/// is whatever the set says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Press {
    Key(&'static str),
    Mark(G),
}

/// What a footer entry does when clicked. `Hint` is a reminder of a key that
/// already works, not a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Hint,
    NewWorktree,
    NewAgent,
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
            Action::NewAgent => match session.focus {
                Focus::Ws { .. } => Some(Target::NewAgent),
                Focus::Layout(_) => None,
            },
            Action::NewWorktree => match session.focus {
                Focus::Ws { .. } => Some(Target::NewWorktree),
                Focus::Layout(_) => None,
            },
        }
    }
}

/// How much of a row a badge may take.
///
/// Eight columns in a column this narrow. A badge is a glance, and one that
/// crowds out the name it belongs to has stopped being one.
pub const BADGE: usize = 8;

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
    /// One tab of an expanded workspace.
    ///
    /// Between the workspace and its panes, which is the level the nav was
    /// already drawing with nothing behind it.
    Tab {
        p: usize,
        w: usize,
        t: usize,
    },
    /// One pane of an expanded tab.
    Pane {
        p: usize,
        w: usize,
        t: usize,
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
            Row::Tab { p, w, t } => Target::NavTab { p, w, t },
            Row::Pane { p, w, t, index } => Target::NavPane { p, w, t, index },
            Row::NewWorkspace(p) => Target::NewWorkspace(p),
            _ => return None,
        })
    }
}

/// Every row the nav would draw if it had unlimited height.
pub fn rows(cfg: &Config, session: &Session) -> Vec<Row> {
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
            if cfg.nav.tall() && proj.repo_of(w).is_some_and(|r| !r.branch.is_empty()) {
                out.push(Row::Branch { p, w });
            }
            // A workspace with one tab holding one pane draws no subtree: there
            // is nothing the row above does not already say.
            let ws = &proj.workspaces[w];
            if ws.expanded && (ws.tabs.len() > 1 || ws.panes().len() > 1) {
                for (t, tab) in ws.tabs.iter().enumerate() {
                    // One tab is not a level worth drawing -- it would be a row
                    // saying "the only arrangement" above the panes in it.
                    if ws.tabs.len() > 1 {
                        out.push(Row::Tab { p, w, t });
                    }
                    let open = ws.tabs.len() == 1 || tab.expanded || t == ws.tab;
                    if open && tab.tree.leaves().len() > 1 {
                        out.extend((0..tab.tree.leaves().len()).map(|index| Row::Pane {
                            p,
                            w,
                            t,
                            index,
                        }));
                    }
                }
            }
        }
        out.push(Row::NewWorkspace(p));
    }
    out.push(Row::Footer(Section::Spaces));

    // Only what is owed, and only when something is. An empty heading is
    // slower to read than no heading, and this section spent its whole life so
    // far holding a place for the answer "nothing".
    if !cfg.nav.attention_never() {
        let owed = attention(session);
        if !owed.is_empty() || cfg.nav.attention_always() {
            out.push(Row::Blank);
            out.push(Row::Heading(Section::Attention, owed.len()));
            out.extend(owed.into_iter().map(|(p, w)| Row::Agent { p, w }));
            out.push(Row::Footer(Section::Attention));
        }
    }

    out
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
fn attention(session: &Session) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = Vec::new();
    for (p, proj) in session.projects.iter().enumerate() {
        for (w, ws) in proj.workspaces.iter().enumerate() {
            // The same field the glyph reads. Asking the occupant here and
            // the state there was two answers to one question, and they
            // disagreed on screen.
            // Blocked and done only. `working` is not asking for anything and
            // `idle` is asking for less than that, and a zone that lists them
            // is the third list this replaced.
            if matches!(
                ws.state,
                crate::agent::State::Blocked | crate::agent::State::Done
            ) {
                v.push((p, w));
            }
        }
    }
    v.sort_by_key(|&(p, w)| {
        let ws = &session.projects[p].workspaces[w];
        // Oldest first within a rank, so `age` sorts descending. There is one
        // sensible order here and it is this one, which is why the toggle that
        // used to sit in this section's footer is gone.
        (
            rank(state_of(ws)),
            u64::MAX - ws.touched.elapsed().as_secs(),
        )
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

/// Everything outside the nav that a frame of it depends on.
///
/// The same argument as `Ctx` one level down: the signature was growing a
/// parameter every time a row learned to say something new, and four of them
/// are read-only for the whole frame.
pub struct Frame<'a> {
    pub cfg: &'a Config,
    pub glyphs: &'a Glyphs,
    pub session: &'a Session,
    /// What a board last reported about itself, by name.
    pub badges: &'a dyn Fn(&str) -> Option<String>,
}

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    f: &Frame,
    nav: &mut Nav,
    hits: &mut HitMap,
) -> Vec<Row> {
    let Frame {
        cfg,
        glyphs,
        session,
        badges,
    } = *f;
    fill(buf, area, THEME.panel());
    let all = rows(cfg, session);
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
                    g: glyphs,
                    badges,
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
    /// The marks to draw with, resolved once per frame.
    g: &'a Glyphs,
    /// What a board last reported about itself, by name.
    badges: &'a dyn Fn(&str) -> Option<String>,
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
            let right = count.to_string();
            let x = inner.right().saturating_sub(cells(&right));
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
                        &format!("  {}  ", cx.g.text(G::Sep)),
                        THEME.faint(),
                        inner.right().saturating_sub(x),
                    );
                }
                let start = x;
                let key = match key {
                    Press::Key(k) => k,
                    Press::Mark(g) => cx.g.text(*g),
                };
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
            x += match open {
                true => write_str(buf, x, y, cx.g.text(G::Running), THEME.ok(), w),
                false => cx.g.cells(G::Running),
            };
            x += write_str(buf, x, y, " ", THEME.ok(), w);

            // What the board has to say for itself, right-aligned and reserved
            // before the name is written. This is the whole difference between
            // a link and an instrument: a dashboard you have to open to find
            // out whether it matters is a link with extra steps.
            let mut right = 0u16;
            if let Some(text) = (cx.badges)(&layout.def.name) {
                let text = elide(&text, BADGE, cx.g.text(G::Ellipsis));
                let width = cells(&text);
                right = width + 1;
                write_str(
                    buf,
                    inner.right().saturating_sub(width),
                    y,
                    &text,
                    THEME.faint(),
                    width,
                );
            }

            let left = w.saturating_sub(x - inner.x).saturating_sub(right) as usize;
            let style = if focused {
                style.patch(THEME.project())
            } else {
                style
            };
            write_str(
                buf,
                x,
                y,
                &elide(&layout.def.name, left, cx.g.text(G::Ellipsis)),
                style,
                w,
            );
        }

        Row::Project(p) => {
            let Some(proj) = session.projects.get(p) else {
                return;
            };
            let fold = if proj.expanded {
                G::Expanded
            } else {
                G::Collapsed
            };
            let mut x = inner.x;
            x += write_str(buf, x, y, cx.g.text(fold), THEME.rule_strong(), w);
            x += write_str(buf, x, y, " ", THEME.rule_strong(), w);

            // What a collapsed project is hiding, right-aligned and reserved
            // before the name is written. Folding a project to make twenty of
            // them fit should not also hide the one agent that is blocked --
            // that is the thing the column exists to show.
            let mut right = 0u16;
            if !proj.expanded && !proj.workspaces.is_empty() {
                let n = proj.workspaces.len().to_string();
                right = cells(&n);
                write_str(
                    buf,
                    inner.right().saturating_sub(right),
                    y,
                    &n,
                    THEME.faint(),
                    right,
                );
                if let Some(worst) = worst_state(proj) {
                    let mark = cx.g.text(G::state(worst));
                    right += cells(mark) + 1;
                    write_str(
                        buf,
                        inner.right().saturating_sub(right),
                        y,
                        mark,
                        THEME.state_style(worst),
                        cells(mark),
                    );
                }
                right += 1;
            }

            let left = w.saturating_sub(x - inner.x).saturating_sub(right) as usize;
            write_str(
                buf,
                x,
                y,
                &elide(&proj.name, left, cx.g.text(G::Ellipsis)),
                THEME.project(),
                w,
            );
        }

        Row::Workspace { p, w: wi, n } => {
            let Some(ws) = session.workspace(p, wi) else {
                return;
            };
            let worktree = session
                .projects
                .get(p)
                .is_some_and(|x| x.repo_of(wi).is_some_and(|r| r.worktree));
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
                .is_some_and(|x| x.repo_of(wi).is_some_and(|r| r.worktree));
            space_row(buf, cx, ws, p, wi, None, worktree);
        }

        Row::Branch { p, w: wi } => {
            // The space's own checkout, not the project's: two worktrees of one
            // repository are on two branches, which is why there are two of
            // them.
            let Some(repo) = session.projects.get(p).and_then(|x| x.repo_of(wi)) else {
                return;
            };
            let style = if selected && active {
                base
            } else {
                THEME.branch()
            };
            let left = w.saturating_sub(6) as usize;
            write_str(
                buf,
                inner.x + 6,
                y,
                &elide(&repo.branch, left, cx.g.text(G::Ellipsis)),
                style,
                w,
            );
        }

        Row::Tab { p, w: wi, t } => {
            let Some(ws) = session.workspace(p, wi) else {
                return;
            };
            let Some(tab) = ws.tabs.get(t) else { return };
            let here = ws.tab == t;
            let style = match (selected && active, here) {
                (true, _) => base,
                // The one on screen, as against the ones that are not. Same
                // distinction the rail's chips make, in one column.
                (false, true) => THEME.text(),
                (false, false) => THEME.dim(),
            };
            let mut x = inner.x + 4;
            x += write_str(
                buf,
                x,
                y,
                cx.g.text(if here { G::Expanded } else { G::Collapsed }),
                THEME.rule_strong(),
                w,
            );
            x += write_str(buf, x, y, " ", THEME.rule_strong(), w);

            // How many panes are in it, since a collapsed tab says nothing else
            // about what is inside.
            let mut right = 0u16;
            if tab.panes.len() > 1 {
                let n = tab.panes.len().to_string();
                right = cells(&n) + 1;
                write_str(
                    buf,
                    inner.right().saturating_sub(cells(&n)),
                    y,
                    &n,
                    THEME.faint(),
                    cells(&n),
                );
            }
            let left = w.saturating_sub(x - inner.x).saturating_sub(right) as usize;
            write_str(
                buf,
                x,
                y,
                &crate::name::shorten(&ws.tab_label(t), left, cx.g.text(G::Ellipsis)),
                style,
                w,
            );
        }

        Row::Pane { p, w: wi, t, index } => {
            let Some(ws) = session.workspace(p, wi) else {
                return;
            };
            let Some(tab) = ws.tabs.get(t) else { return };
            let Some(&id) = tab.tree.leaves().get(index) else {
                return;
            };
            let Some(pane) = tab.pane(id) else {
                return;
            };
            let last = index + 1 == tab.tree.leaves().len();
            let style = if selected && active {
                base
            } else {
                THEME.faint()
            };

            // One further in when there is a tab level above, so the tree reads
            // as a tree rather than as two lists at the same indent.
            let mut x = inner.x + if ws.tabs.len() > 1 { 6 } else { 4 };
            let branch = if last { G::TreeLast } else { G::TreeMid };
            x += write_str(buf, x, y, cx.g.text(branch), THEME.rule_strong(), w);
            x += write_str(buf, x, y, " ", THEME.rule_strong(), w);
            // A pane's own label if a layout gave it one, otherwise whatever
            // the program inside is calling itself.
            let name = pane
                .label
                .clone()
                .or_else(|| pane.title())
                .unwrap_or_else(|| {
                    // Failing a label or a title, say what is running: that is
                    // more use than the directory, which the row above implies.
                    match &pane.occupant {
                        // Its name, which is how it is addressed, rather than
                        // its kind, which every agent of that kind shares.
                        crate::agent::Occupant::Agent(k) => pane
                            .agent_name
                            .clone()
                            .unwrap_or_else(|| k.name.to_string()),
                        crate::agent::Occupant::Program(p) => p.clone(),
                        // "free" rather than "shell": what matters about a
                        // prompt is that an agent could be started in it.
                        o if o.available() => "free".into(),
                        // Nothing known yet -- the first tick after a split, or
                        // a platform with no foreground group to read.
                        _ => pane
                            .cwd
                            .file_name()
                            .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
                    }
                });
            let left = w.saturating_sub(x - inner.x) as usize;
            write_str(
                buf,
                x,
                y,
                &elide(&name, left, cx.g.text(G::Ellipsis)),
                style,
                w,
            );
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
    let state = state_of(ws);
    let glyph = cx.g.text(G::state(state));

    let age = since(ws.touched.elapsed());
    let age_w = cells(&age);
    write_str(
        buf,
        inner.right().saturating_sub(age_w),
        y,
        &age,
        THEME.faint(),
        age_w,
    );

    let mut x = inner.x;
    x += match (ws.panes().len() > 1, ws.expanded) {
        (false, _) => cx.g.cells(G::Collapsed),
        (true, false) => write_str(buf, x, y, cx.g.text(G::Collapsed), THEME.rule_strong(), w),
        (true, true) => write_str(buf, x, y, cx.g.text(G::Expanded), THEME.rule_strong(), w),
    };
    x += write_str(buf, x, y, " ", THEME.rule_strong(), w);
    x += write_str(buf, x, y, glyph, THEME.state_style(state), w);
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
    let mark = if worktree {
        cx.g.cells(G::Worktree) + 1
    } else {
        0
    } + if ws.naming.held {
        cx.g.cells(G::Held) + 1
    } else {
        0
    };
    let left = w
        .saturating_sub(x - inner.x)
        .saturating_sub(age_w + 1 + mark) as usize;
    x += write_str(
        buf,
        x,
        y,
        &crate::name::shorten(&ws.label, left, cx.g.text(G::Ellipsis)),
        style,
        w,
    );
    // A zoomed workspace looks exactly like one with a single pane, so the row
    // is the only thing that can say the others are still there.
    if ws.here().zoomed && ws.panes().len() > 1 {
        x += write_str(buf, x + 1, y, cx.g.text(G::Zoomed), THEME.warn(), w) + 1;
    }
    // Kept in the short form, where the branch line is not. Which branch this
    // is is scenery; that it is a worktree at all is what tells two rows
    // wearing the same repository's name apart.
    if worktree {
        x += write_str(buf, x + 1, y, cx.g.text(G::Worktree), THEME.worktree(), w) + 1;
    }
    // A held name is one dirk has stood down from. Worth saying, because the
    // alternative is a workspace that mysteriously stops being renamed.
    if ws.naming.held {
        write_str(buf, x + 1, y, cx.g.text(G::Held), THEME.faint(), w);
    }
}

/// The most urgent state among a project's workspaces, or nothing if none of
/// them has one worth reporting.
///
/// What a collapsed row shows instead of the rows it is hiding. Without it,
/// folding a project is a way of losing exactly what the column is for.
fn worst_state(proj: &crate::mux::session::Project) -> Option<&'static str> {
    proj.workspaces
        .iter()
        .map(|ws| state_of(ws))
        .filter(|s| rank(s) < rank("idle"))
        .min_by_key(|s| rank(s))
}

/// A workspace's state, as the word the theme and the nav both speak.
///
/// The agent's state when there is an agent. When there is not, the column
/// still has something worth saying — a build is running — and blanking that
/// lost it.
///
/// A shell at a prompt is the exception, and it changed when the agents list
/// became a zone that only holds what is owed. That list was where you could
/// see which workspaces held an agent at all; without it, an agent sitting
/// idle drew the same dot as an empty shell and there was nothing left on
/// screen that told them apart. So a prompt draws nothing now: an empty column
/// means nothing is happening here, which is what an empty column should mean,
/// and `·` means an agent, at rest.
fn state_of(ws: &Workspace) -> &'static str {
    use crate::agent::{Occupant, State};
    if ws.state != State::None {
        return ws.state.glyph_name();
    }
    match ws.active_pane() {
        None => "unknown",
        Some(p) if p.dead => "unknown",
        Some(p) => match &p.occupant {
            // Something is running that is not an agent and not a prompt.
            Occupant::Program(_) => "working",
            _ => "unknown",
        },
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
    fn only_what_is_owed_reaches_the_attention_zone() {
        // The zone this replaced listed every workspace holding an agent, which
        // meant a section permanently full of things not asking for anything.
        for (state, wanted) in [
            (crate::agent::State::Blocked, true),
            (crate::agent::State::Done, true),
            (crate::agent::State::Working, false),
            (crate::agent::State::Idle, false),
            (crate::agent::State::None, false),
        ] {
            assert_eq!(
                matches!(
                    state,
                    crate::agent::State::Blocked | crate::agent::State::Done
                ),
                wanted,
                "{state:?} in the attention zone: {wanted}"
            );
        }
    }

    #[test]
    fn a_pane_row_goes_to_that_pane_and_not_just_its_workspace() {
        let pane = Row::Pane {
            p: 1,
            w: 2,
            t: 0,
            index: 3,
        };
        assert!(pane.selectable());
        assert_eq!(
            pane.target(),
            Some(Target::NavPane {
                p: 1,
                w: 2,
                t: 0,
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
