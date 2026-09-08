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

//! The bottom bar: the one surface that runs the full width of the terminal.
//!
//! It is the only thing in dirk that spans sidebar and panes both, which is
//! what makes it the footer rather than a pane: a pane can only ever start
//! where the sidebar ends. One row, always, in every state — a footer that
//! disappears when there is nothing to say is not a footer.
//!
//! **Three zones, the way the nav has three zones.** Fixed roles at fixed
//! anchors, so the eye learns where to look instead of reading a sentence.
//!
//! *Left, identity.* Who you are, which session, and which machine. The slot
//! used to hold the product's name — constant, unclickable, and an answer to a
//! question nobody had. What it answers now can be wrong, which is the point.
//!
//! *Middle, context and attention.* Where you are inside the session, and —
//! anchored hard against the exits so it never moves — what is owed. Empty
//! when nothing is owed, and that emptiness is the fastest way to say so.
//!
//! *Right, the exits.* Detach takes the corner because the corner is the
//! cheapest target a pointer has and detaching is the safe, frequent one. Quit
//! sits inboard behind a gap, and still takes two clicks.
//!
//! **The middle shows what the nav is not showing.** That is one rule, not two
//! modes: with the nav up it is where you are, down to the pane, which nothing
//! else names; with the nav hidden it is the list, because then the rail is the
//! only navigation there is.
//!
//! **What it gives up under pressure is a list, in order.** Each step gives up
//! strictly less than the one before it, so the bar shrinks monotonically and
//! can never trade something needed for something not. Attention is not on the
//! list.

use crate::config::Config;
use crate::glyph::G;
use crate::hit::{HitMap, Target};
use crate::mux::{Focus, Session};
use crate::theme::THEME;
use crate::ui::{cells, fill, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

/// What the rail should say right now, as opposed to what it always says.
pub struct Now<'a> {
    pub clock: &'a str,
    /// How far back the focused pane is being read. Zero is the live screen.
    ///
    /// Its own field rather than folded into the note: a note is transient and
    /// this is a state you can sit in for a minute, and a pane being read from
    /// the past looks exactly like a program that has stopped.
    pub scrolled: usize,
    /// A transient note. Empty most of the time.
    pub note: &'a str,
    /// The prefix is down and the next key is a command.
    ///
    /// Separate from the note because it is not one. It is the most
    /// time-critical thing the interface says, and it is the only thing that
    /// displaces where you are.
    pub prefix: bool,
    /// The quit button has been clicked once and is waiting for the second.
    pub quit_armed: bool,
    /// This session's name, whatever it is called.
    pub session: Option<&'a str>,
    /// Whether the nav is on screen, which decides what the middle holds.
    pub nav_visible: bool,
}

/// What the rail gives up, in the order it gives it up.
///
/// One list, applied cumulatively: at step *n* everything before *n* is gone.
/// The alternative is what was here before — each part measured in whatever
/// order the code happened to run, which is how the bar came to keep the clock
/// and drop the count of agents waiting for a human.
#[derive(Clone, Copy, PartialEq)]
enum Give {
    /// Ambient. Every terminal has one.
    Clock,
    /// `2/3` is a nicety.
    PaneCounter,
    /// The nav still says which project this is.
    Project,
    /// There is rarely more than one session.
    SessionName,
    /// `detach` and `quit` become their marks.
    ExitWords,
    /// The intent goes. The nav has it, unless the nav is what is gone.
    Context,
    /// Late: being on another machine is a hazard, not decoration.
    Host,
    /// The mark alone.
    Name,
    // Below the floor. Nothing above got the bar to fit, so what is owed
    // starts giving up its quieter halves — and `blocked`, the only state
    // waiting on a human, is the last thing on the screen.
    ScrollDepth,
    DoneCount,
}

const LADDER: &[Give] = &[
    Give::Clock,
    Give::PaneCounter,
    Give::Project,
    Give::SessionName,
    Give::ExitWords,
    Give::Context,
    Give::Host,
    Give::Name,
    Give::ScrollDepth,
    Give::DoneCount,
];

/// One drawable run: its text, how it is drawn, and what clicking it means.
struct Seg {
    text: String,
    style: Style,
    hit: Option<Target>,
}

impl Seg {
    fn new(text: impl Into<String>, style: Style) -> Seg {
        Seg {
            text: text.into(),
            style,
            hit: None,
        }
    }
    fn on(mut self, target: Target) -> Seg {
        self.hit = Some(target);
        self
    }
    /// The rail's ground, with the role's colour on top of it.
    ///
    /// The order matters and is the opposite of what it reads like: `patch`
    /// lets the *argument* win, so `style.patch(rail)` hands every role the
    /// rail's own foreground and flattens the bar to one colour. It did, for
    /// as long as this file has existed -- the hierarchy this is all for was
    /// inert, and a shot of it is seven runs of the same `#d3ebe9`.
    fn plain(text: impl Into<String>, style: Style) -> Seg {
        Seg::new(text, THEME.rail().patch(style))
    }
    fn gap(n: usize) -> Seg {
        Seg::plain(" ".repeat(n), THEME.rail())
    }
}

fn width(segs: &[Seg]) -> u16 {
    segs.iter().map(|s| cells(&s.text)).sum()
}

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    cfg: &Config,
    g: &crate::glyph::Glyphs,
    session: &Session,
    now: &Now,
    hits: &mut HitMap,
) {
    fill(buf, area, THEME.rail());
    let bar = Bar {
        cfg,
        g,
        session,
        now,
        who: cfg.identity.who(),
        host: cfg.identity.host_shown(),
        flat: session.flat(),
    };

    // The first arrangement that fits. Walking forward means the first one
    // found is also the fullest, because every step gives up more than the
    // last.
    for step in 0..=LADDER.len() {
        let Some((left, right)) = bar.build(&LADDER[..step], area.width) else {
            continue;
        };
        let (lw, rw) = (width(&left), width(&right));
        // Two cells of ground between the halves, or they read as one run.
        // The left half starts one column in, and two columns of ground sit
        // between the halves: 1 + lw + 2 + rw. Reserving only lw + rw + 2
        // delivered one column of ground at the tightest fit, not the two the
        // comment promised.
        if lw + rw + 3 <= area.width {
            // Each half is clipped to its own width rather than to the bar's.
            // They cannot collide -- that is what the check above is for --
            // but a budget that is larger than the space is a budget that
            // stops catching the day it can.
            paint(buf, area.x + 1, area.y, &left, lw, hits);
            paint(
                buf,
                area.right().saturating_sub(rw),
                area.y,
                &right,
                rw,
                hits,
            );
            return;
        }
    }

    // Below the floor. The mark says which program this is, in the column the
    // bar starts in, and nothing else is true enough to be worth a cell.
    write_str(
        buf,
        area.x.saturating_add(1).min(area.right().saturating_sub(1)),
        area.y,
        bar.mark(),
        THEME.rail().patch(THEME.working()),
        area.width.saturating_sub(1),
    );
}

fn paint(buf: &mut Buffer, x: u16, y: u16, segs: &[Seg], max: u16, hits: &mut HitMap) {
    let mut at = x;
    for seg in segs {
        let w = cells(&seg.text);
        write_str(buf, at, y, &seg.text, seg.style, max.saturating_sub(at - x));
        if let Some(target) = seg.hit {
            hits.push(
                Rect {
                    x: at,
                    y,
                    width: w,
                    height: 1,
                },
                target,
            );
        }
        at += w;
    }
}

struct Bar<'a> {
    cfg: &'a Config,
    g: &'a crate::glyph::Glyphs,
    session: &'a Session,
    now: &'a Now<'a>,
    // Worked out once per frame rather than once per step. `build` runs up to
    // eleven times looking for a fit, and these are two environment lookups, a
    // `gethostname(2)` and a walk of every workspace -- which the rail would
    // otherwise pay for eleven times over on every redraw of a busy pane.
    who: String,
    host: Option<String>,
    flat: Vec<(usize, usize)>,
}

impl Bar<'_> {
    fn mark(&self) -> &str {
        match self.cfg.identity.mark.is_empty() {
            true => self.g.text(G::Brand),
            false => self.cfg.identity.mark.as_str(),
        }
    }

    /// The bar at one step of the ladder, in `room` columns.
    ///
    /// Everything except the middle has a size of its own. The middle takes
    /// what is left and shortens itself into it, which is why the intent goes
    /// from `2 Reading the vt100 grid` to `2 …vt100 grid` before it goes at
    /// all: dropping a thing outright when it could have been shortened gives
    /// up more than the step above it, and the whole point of the list is that
    /// it never does that.
    /// `None` when the middle cannot be drawn legibly at this step, so the
    /// ladder takes another one. Without that, shortening a label to `2 R…`
    /// counts as fitting, and the steps that would have dropped the project
    /// and given the name room back never run.
    fn build(&self, gone: &[Give], room: u16) -> Option<(Vec<Seg>, Vec<Seg>)> {
        let up = |g: Give| gone.contains(&g);

        // ── Left: identity, then what the nav is not showing ─────────────
        let mut left = vec![Seg::plain(
            self.mark(),
            THEME.working().add_modifier(ratatui::style::Modifier::BOLD),
        )];
        if !up(Give::Name) {
            left.push(Seg::gap(1));
            left.push(Seg::plain(self.who.clone(), THEME.project()));
            if !up(Give::Host)
                && let Some(host) = &self.host
            {
                left.push(Seg::plain("@", THEME.faint()));
                left.push(Seg::plain(host.clone(), THEME.worktree()));
            }
            if !up(Give::SessionName)
                && let Some(name) = self.cfg.identity.session_shown(self.now.session)
            {
                left.push(Seg::plain(
                    format!(" {} ", self.g.text(G::Sep)),
                    THEME.faint(),
                ));
                left.push(Seg::plain(name, THEME.branch()));
            }
        }

        // ── Right: the clock, what is owed, and the two ways out ─────────
        //
        // What is owed sits between them rather than before the clock, so it
        // is always exactly the width of the exits from the right edge. Put it
        // the other side and its column moves by seven the moment the clock is
        // given up -- and a thing you are meant to catch out of the corner of
        // your eye has to be in the same place every time.
        let mut right = Vec::new();
        if !up(Give::Clock) {
            right.push(Seg::plain(self.now.clock, THEME.faint()));
        }
        let owed = self.attention(gone);
        if !owed.is_empty() {
            if !right.is_empty() {
                right.push(Seg::gap(2));
            }
            right.extend(owed);
        }
        if !right.is_empty() {
            right.push(Seg::gap(2));
        }
        right.extend(self.exits(gone));

        // The column the bar starts in, two of ground between the halves, and
        // three before the middle.
        let taken = width(&left) + width(&right) + 1 + 2 + 3;
        let middle = self.middle(gone, room.saturating_sub(taken))?;
        if !middle.is_empty() {
            left.push(Seg::gap(3));
            left.extend(middle);
        }
        Some((left, right))
    }

    /// The middle: what the nav is not showing.
    ///
    /// One rule and not two modes. With the nav up the list is right there, so
    /// this is the one thing the list does not carry: exactly where you are,
    /// down to the pane. With the nav hidden nothing else is showing the
    /// session, so this becomes the list.
    ///
    /// A chord displaces both. Mid-chord nothing else on the bar matters, and
    /// it is the only thing in the interface that is over in half a second.
    fn middle(&self, gone: &[Give], room: u16) -> Option<Vec<Seg>> {
        // A chord outranks everything, including its own room: three columns,
        // and if there are not three it draws nothing rather than pushing the
        // way out off the bar for half a second.
        if self.now.prefix {
            let mark = format!(" {} ", self.g.text(G::Brand));
            // `None`, not an empty middle: returning nothing would make *this*
            // step succeed and stop the ladder, so the indicator would be
            // dropped while the clock and the full-word exits were still on
            // the bar. A chord outranks both, and this is how it says so.
            return (cells(&mark) <= room).then(|| vec![Seg::new(mark, THEME.armed())]);
        }
        if gone.contains(&Give::Context) {
            return Some(Vec::new());
        }
        // A note is a sentence, so it is cut at the end rather than the front,
        // and it is cut rather than left whole: a note that will not fit used
        // to be returned at full width anyway, and then no step of the ladder
        // could fit either — so a narrow terminal in copy mode lost the way
        // out, the clock and what was owed, and drew the mark alone.
        if !self.now.note.is_empty() {
            let text = crate::ui::elide(self.now.note, room as usize, self.g.text(G::Ellipsis));
            if cells(&text) < Self::NOTE_FLOOR.min(cells(self.now.note)) {
                return None;
            }
            return Some(vec![Seg::plain(text, THEME.dim())]);
        }
        match self.now.nav_visible {
            true => self.breadcrumb(gone, room),
            false => self.chips(room),
        }
    }

    /// The shortest name still worth the columns. Below it there is nothing a
    /// reader could tell from another name, and a lone stub is a column spent
    /// saying nothing.
    const NAME_FLOOR: u16 = 10;
    /// A note is a sentence and survives being cut short better than a name
    /// does, but not by much.
    const NOTE_FLOOR: u16 = 12;

    fn breadcrumb(&self, gone: &[Give], room: u16) -> Option<Vec<Seg>> {
        let Focus::Ws { p, w } = self.session.focus else {
            return Some(Vec::new());
        };
        let (Some(project), Some(ws)) =
            (self.session.projects.get(p), self.session.workspace(p, w))
        else {
            return Some(Vec::new());
        };
        let crumb = || Seg::plain(format!(" {} ", self.g.text(G::Crumb)), THEME.faint());

        let mut out = Vec::new();
        if !gone.contains(&Give::Project) {
            out.push(Seg::plain(project.name.clone(), THEME.dim()));
            out.push(crumb());
        }

        // The number the nav reads by and the chips jump by. Without it a
        // workspace is named by its intent alone, and two that have not been
        // named yet are both called after the project -- so the bar could not
        // say which of them you were in, which is the question it exists to
        // answer.
        if let Some(n) = self.flat.iter().position(|&at| at == (p, w)) {
            out.push(Seg::plain(format!("{} ", n + 1), THEME.number()));
        }
        // `1/1` is a fact about a split that is not there.
        let (at, of) = ws.pane_position();
        let counter = (!gone.contains(&Give::PaneCounter) && of > 1)
            .then(|| vec![crumb(), Seg::plain(format!("{at}/{of}"), THEME.faint())]);

        // The label takes what the rest of the crumb leaves, and refuses the
        // job if what comes out is not enough to read.
        //
        // The test is on what is drawn, not on the budget. Shortening lands on
        // a word boundary, so ten columns of room can produce five columns of
        // name -- and measuring the room rather than the name is how fifty
        // columns came to show less than forty-four did, by keeping the
        // project and spending the difference on nothing.
        let spent = width(&out) + counter.as_deref().map_or(0, width);
        let budget = room.checked_sub(spent)?;
        let text = crate::name::shorten(&ws.label, budget as usize, self.g.text(G::Ellipsis));
        if cells(&text) < Self::NAME_FLOOR.min(cells(&ws.label)) {
            return None;
        }
        out.push(Seg::plain(text, THEME.text()));
        out.extend(counter.unwrap_or_default());
        Some(out)
    }

    /// Every workspace, numbered the way the nav reads: top to bottom.
    fn chips(&self, room: u16) -> Option<Vec<Seg>> {
        let mut out: Vec<Seg> = Vec::new();
        for (n, &(p, w)) in self.flat.iter().enumerate() {
            let Some(ws) = self.session.workspace(p, w) else {
                continue;
            };
            let focused = self.session.focus == Focus::Ws { p, w };
            // A filled bar for the chip you are in and a thin one otherwise:
            // the same distinction the nav makes with its selection row, in
            // one column.
            let (bar, style) = match focused {
                true => (G::BarFocused, THEME.text()),
                false => (G::BarPlain, THEME.dim()),
            };
            let chip = vec![
                Seg::plain(self.g.text(bar), THEME.working()),
                Seg::plain(
                    format!(
                        "{} {}",
                        n + 1,
                        crate::name::shorten(&ws.label, 14, self.g.text(G::Ellipsis))
                    ),
                    style,
                )
                .on(Target::Workspace { p, w }),
            ];
            // As many as fit, and no half of one. A chip cut off at the column
            // is a jump target you cannot read and might still click.
            let gap = if out.is_empty() { 0 } else { 2 };
            if width(&out) + gap + width(&chip) > room {
                break;
            }
            if gap > 0 {
                out.push(Seg::gap(gap as usize));
            }
            out.extend(chip);
        }
        // Room for none of them is not an empty list; it is a step of the
        // ladder that has not been taken yet.
        (!out.is_empty()).then_some(out)
    }

    /// What is owed, and nothing else.
    ///
    /// Anchored against the exits rather than placed after whatever came
    /// before it, so it occupies the same columns at every width and in every
    /// state. Empty when nothing is owed — which is the fastest possible way
    /// to say that nothing needs you, and why there is no count of spaces
    /// here: the nav lists them, and a number that is always there is a middle
    /// that can never be empty.
    fn attention(&self, gone: &[Give]) -> Vec<Seg> {
        let (blocked, done) = self.session.counts();
        let mut out: Vec<Seg> = Vec::new();
        fn push(seg: Seg, out: &mut Vec<Seg>) {
            if !out.is_empty() {
                out.push(Seg::gap(2));
            }
            out.push(seg);
        }

        // Blocked first, and it is the only thing here that survives every
        // step of the ladder. It is the one state waiting on a human.
        if blocked > 0 {
            push(
                Seg::new(
                    format!(" {} {blocked} ", self.g.text(G::Blocked)),
                    THEME.alarm(),
                )
                .on(Target::Attention(crate::agent::State::Blocked)),
                &mut out,
            );
        }
        if done > 0 && !gone.contains(&Give::DoneCount) {
            push(
                Seg::plain(format!("{} {done}", self.g.text(G::Done)), THEME.ok())
                    .on(Target::Attention(crate::agent::State::Done)),
                &mut out,
            );
        }
        // Being read from the past is a state you can sit in and forget, and it
        // is not the clock's neighbour: the keys mean different things and the
        // screen is not live.
        if self.now.scrolled > 0 && !gone.contains(&Give::ScrollDepth) {
            push(
                Seg::plain(
                    format!("{}{}", self.g.text(G::Back), self.now.scrolled),
                    THEME.worktree(),
                ),
                &mut out,
            );
        }
        out
    }

    /// The two ways out.
    ///
    /// In words rather than one mark, because since a session outlives its
    /// terminal these are different things and a glyph cannot say which:
    /// detaching parks the work, quitting ends every shell and every agent.
    ///
    /// Detach has the corner. The bottom-right is the cheapest target a
    /// pointer has — you can throw the mouse at it and it cannot overshoot —
    /// and giving that to the thing that ends a day's work was backwards. Quit
    /// sits inboard behind a gap so that overshooting one no longer lands on
    /// the other, and it still takes two clicks.
    fn exits(&self, gone: &[Give]) -> Vec<Seg> {
        let words = !gone.contains(&Give::ExitWords);
        let quit = match (self.now.quit_armed, words) {
            (true, _) => " quit? ".to_string(),
            (false, true) => format!(" {} quit ", self.g.text(G::Close)),
            (false, false) => format!(" {} ", self.g.text(G::Close)),
        };
        let detach = match words {
            true => " detach ".to_string(),
            false => format!(" {} ", self.g.text(G::Detach)),
        };
        vec![
            match self.now.quit_armed {
                true => Seg::new(quit, THEME.alarm()),
                false => Seg::plain(quit, THEME.dim()),
            }
            .on(Target::Quit),
            Seg::gap(2),
            // Full strength, not dimmed to match: leaving is the ordinary
            // thing you do several times a day and should not look like the
            // dangerous one's quieter sibling.
            Seg::plain(detach, THEME.text()).on(Target::Detach),
        ]
    }
}
