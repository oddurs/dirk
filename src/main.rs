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

//! dirk — a multiplexer that knows what its sessions are for.
//!
//! The shape of the thing, in one screen:
//!
//! ```text
//! ┌──────────────┬──────────────────────────────────────┐
//! │  P A G E S   │                                      │
//! │  1 • ptop    │                                      │
//! │  2   lazygit │            the focused pane          │
//! │  3   cairn   │                                      │
//! │              │                                      │
//! │ P R O J E…   │                                      │
//! │ ▾ dirk       │                                      │
//! │   * mux core │                                      │
//! │   · shell    │                                      │
//! │   + workspace│                                      │
//! ├──────────────┴──────────────────────────────────────┤
//! │ ◆ dirk  ▊1 mux core ▏2 shell        2 spaces  14:22 │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! One event loop, one channel, three kinds of producer: the terminal reader,
//! one thread per pane, and a ticker. The loop blocks on a receive and drains
//! whatever else has queued before it draws, so a pane spewing output costs one
//! frame rather than one frame per write, and an idle dirk costs nothing at
//! all. There is no polling anywhere in this file.

mod agent;
mod config;
mod git;
mod hit;
mod keys;
mod llm;
mod mux;
mod name;
mod notify;
mod theme;
mod tokens;
mod ui;

use config::Config;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use hit::{HitMap, Target};
use mux::{Dir, Ev, Focus, Pane, Session};
use ratatui::Frame;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::{Terminal, widgets::Widget};
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use theme::THEME;
use ui::nav::{Nav, Row};
use ui::picker::Picker;

const USAGE: &str = "\
Usage: dirk [OPTION]...
Run a terminal multiplexer with a clickable project tree, static layouts, and
workspaces named from what the program inside them says it is doing.

  -h, --help     display this help and exit
  -V, --version  output version information and exit

dirk reads ~/.config/dirk/config.toml when it exists, and runs on built-in
defaults when it does not.  The prefix key is Ctrl-Space; press it and then 'q'
to quit.

Report bugs to: <https://github.com/oddurs/dirk/issues>
";

const VERSION: &str = concat!(
    "dirk ",
    env!("CARGO_PKG_VERSION"),
    "\n",
    "Copyright (C) 2026 Oddur Sigurdsson\n",
    "License GPLv3+: GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>.\n",
    "This is free software: you are free to change and redistribute it.\n",
    "There is NO WARRANTY, to the extent permitted by law.\n",
    "\n",
    "Written by Oddur Sigurdsson.\n",
);

fn main() -> io::Result<()> {
    // Options are answered before the terminal is touched, so `dirk --version`
    // in a pipe behaves like any other program rather than briefly taking over
    // the screen. dirk takes at most one, and every one of them exits.
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                print!("{VERSION}");
                return Ok(());
            }
            other => {
                eprintln!("dirk: unrecognized option '{other}'");
                eprintln!("Try 'dirk --help' for more information.");
                std::process::exit(1);
            }
        }
    }

    let cfg = Config::load();

    let (tx, rx) = mpsc::channel::<Ev>();
    spawn_input(tx.clone());
    spawn_ticker(tx.clone());

    let mut terminal = setup()?;
    let size = terminal.size()?;

    let session = Session::new(&cfg, tx.clone());
    let mut app = App::new(cfg, session, size, tx);
    app.bootstrap();

    let result = app.run(&mut terminal, rx);
    restore();
    result
}

// ── Terminal lifecycle ──────────────────────────────────────────────────

type Term = Terminal<CrosstermBackend<io::Stdout>>;

/// Narrower than this and a row cannot show a name, which is the only reason
/// the nav is there.
const MIN_NAV: u16 = 20;

fn setup() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;

    // A panic in raw mode leaves the terminal unusable and the backtrace
    // unreadable. Restore first, then let the default hook print.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        default(info);
    }));

    Terminal::new(CrosstermBackend::new(io::stdout()))
}

fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
}

fn spawn_input(tx: Sender<Ev>) {
    std::thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            if tx.send(Ev::Term(ev)).is_err() {
                return;
            }
        }
    });
}

/// One tick a second: the clock needs it, and so does naming — a title that
/// went quiet mid-debounce has no further output to wake the loop with.
fn spawn_ticker(tx: Sender<Ev>) {
    std::thread::spawn(move || {
        while tx.send(Ev::Tick).is_ok() {
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

// ── App ─────────────────────────────────────────────────────────────────

struct App {
    cfg: Config,
    session: Session,
    hits: HitMap,
    /// The prefix key has been pressed and the next key is a command.
    prefix: bool,
    picker: Option<Picker>,
    sidebar: bool,
    /// A transient note in the rail, cleared on the next tick that finds it
    /// stale.
    status: String,
    status_at: Instant,
    /// Where panes live, kept so a pane can be spawned at the right size
    /// before it has ever been drawn.
    content: Rect,
    /// Kept so background work -- a git read, say -- can post its answer back
    /// to the one loop that owns the state.
    tx: Sender<Ev>,
    /// Where the nav is looking, and what it last drew. The rows are kept so a
    /// keystroke can act on the same list the pointer sees.
    nav: Nav,
    nav_rows: Vec<Row>,
    side: Rect,
    /// When the quit button was armed. Quitting ends every shell and agent in
    /// the session, and the button sits at the edge of the screen where a stray
    /// click is most likely, so it takes two.
    quit_armed: Option<Instant>,
    /// True while a process-table sample is in flight.
    sampling: bool,
    /// True while the pointer is dragging the divider. Held as state because a
    /// drag is three events and only the first one lands on the divider.
    dragging: bool,
    quit: bool,
}

impl App {
    fn new(cfg: Config, session: Session, size: ratatui::layout::Size, tx: Sender<Ev>) -> Self {
        let width = size.width;
        let height = size.height;
        let sidebar_w = cfg.sidebar_width;
        Self {
            cfg,
            session,
            hits: HitMap::default(),
            prefix: false,
            picker: None,
            sidebar: true,
            status: String::new(),
            status_at: Instant::now(),
            tx,
            nav: Nav::default(),
            nav_rows: Vec::new(),
            side: Rect::ZERO,
            sampling: false,
            quit_armed: None,
            dragging: false,
            content: Rect {
                x: sidebar_w,
                y: 0,
                width: width.saturating_sub(sidebar_w),
                height: height.saturating_sub(1),
            },
            quit: false,
        }
    }

    /// Open something so dirk does not start on an empty screen. The directory
    /// you launched from is the obvious guess and almost always the right one.
    fn bootstrap(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| config::home());
        let p = self.session.open_project(&cwd);
        let (rows, cols) = (self.content.height, self.content.width);
        self.session.new_workspace(p, rows, cols);
    }

    fn note(&mut self, msg: &str) {
        self.status = msg.to_string();
        self.status_at = Instant::now();
    }

    fn run(&mut self, terminal: &mut Term, rx: Receiver<Ev>) -> io::Result<()> {
        terminal.draw(|f| self.render(f))?;

        while let Ok(ev) = rx.recv() {
            self.handle(ev);
            // Coalesce whatever else has already queued. A pane writing fast
            // produces one redraw, not one per write.
            while let Ok(next) = rx.try_recv() {
                self.handle(next);
            }
            // Once per frame, for the same reason drawing is: one `read` of a
            // busy pane produces an event, and working out every agent's state
            // locks each agent pane's terminal and reads its screen. Doing that
            // per chunk contends with the reader threads holding the same lock.
            let changes = self.session.update_states(Instant::now());
            self.announce(changes);
            if self.quit || self.session.is_empty() {
                return Ok(());
            }
            terminal.draw(|f| self.render(f))?;
        }
        Ok(())
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Term(Event::Key(k)) if k.kind != KeyEventKind::Release => self.on_key(k),
            Ev::Term(Event::Mouse(m)) => self.on_mouse(m),
            Ev::Term(Event::Resize(..)) | Ev::Term(_) => {}
            Ev::Output(id) => {
                self.session.touch(id);
                self.session.track_intents(&self.cfg.naming);
                self.rename_pass();
            }
            Ev::Git(answer) => self.session.apply_repo(answer),
            Ev::Suggested { pane, intent } => self.session.apply_suggestion(pane, intent),
            Ev::Agents(reading) => {
                self.sampling = false;
                self.session.apply_agents(reading);
                self.session.name_agents(&self.cfg.naming);
            }
            Ev::Exited(id) => {
                self.session.reap(id);
                self.session.refocus();
            }
            Ev::Tick => {
                self.read_agents();
                self.read_repos();
                self.session.track_intents(&self.cfg.naming);
                self.ask_for_intents();
                self.rename_pass();
                if !self.status.is_empty()
                    && self.status_at.elapsed() > Duration::from_secs(3)
                    && !self.prefix
                {
                    self.status.clear();
                }
                // An armed button that stays armed is a trap of a different
                // shape: it disarms on its own.
                if self
                    .quit_armed
                    .is_some_and(|t| t.elapsed() > Duration::from_secs(3))
                {
                    self.quit_armed = None;
                }
            }
        }
    }

    /// Interrupt, but only for the transitions worth interrupting for.
    ///
    /// Blocked and finished-unseen, and neither for the workspace you are
    /// looking at — you can already see that one.
    fn announce(&mut self, changes: Vec<mux::session::Change>) {
        if !self.cfg.notify.enabled {
            return;
        }
        let floor = Duration::from_millis(self.cfg.notify.min_interval_ms);
        let now = Instant::now();
        for change in changes {
            if change.focused {
                continue;
            }
            let what = match change.to {
                agent::State::Blocked => "is waiting for you",
                agent::State::Done => "has finished",
                // Starting work and settling down are not interruptions.
                _ => continue,
            };
            if !self.session.may_notify(change.at, change.to, now, floor) {
                continue;
            }
            notify::send(
                format!("{} {what}", change.label),
                self.cfg.brand.name.clone(),
            );
        }
    }

    /// Ask the second source about panes whose title says nothing.
    ///
    /// Off the drawing thread like every other external call, and the answer
    /// arrives as an event. A request that hangs must not be able to stop dirk
    /// redrawing, which is the whole reason for the shape.
    fn ask_for_intents(&mut self) {
        let cfg = &self.cfg.naming.sources.llm;
        if !self.cfg.naming.enabled || !cfg.enabled {
            return;
        }
        for (pane, screen) in self.session.wants_intent(cfg, Instant::now()) {
            let cfg = cfg.clone();
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                if let Some(intent) = crate::llm::suggest(&cfg, &screen) {
                    let _ = tx.send(Ev::Suggested { pane, intent });
                }
            });
        }
    }

    /// Ask what is running in each pane.
    ///
    /// The foreground process groups are read here — a `tcgetpgrp` each, too
    /// cheap to be worth a thread. Turning them into names needs the process
    /// table, which does not belong on the drawing thread: `ps` on a loaded
    /// machine is slow, and a slow `ps` must not be able to stop dirk redrawing.
    fn read_agents(&mut self) {
        // One sample at a time. Without this, a `ps` slower than the tick --
        // which is the loaded machine this is written to survive -- accumulates
        // threads and processes without bound, and lets an old reading land
        // after a newer one and overwrite it.
        if self.sampling {
            return;
        }
        let panes = self.session.foregrounds();
        if panes.is_empty() {
            return;
        }
        self.sampling = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let table = crate::agent::table();
            let panes = panes
                .into_iter()
                .map(|(id, pgid)| {
                    let occupant = match pgid {
                        // Nothing in the foreground: the pane is holding
                        // nothing, and that is news.
                        None => Some(crate::agent::Occupant::Unknown),
                        // A group that is not in the table ended between the
                        // pgid being read and `ps` running. That is not news.
                        Some(pgid) => table.get(&pgid).map(crate::agent::identify),
                    };
                    (id, occupant)
                })
                .collect();
            let _ = tx.send(Ev::Agents(crate::agent::Reading { panes }));
        });
    }

    /// Ask git about any project whose answer has gone stale.
    ///
    /// One thread per read, and the answer comes back as an event. A `git` that
    /// has gone to a network remote or is waiting on an index lock would
    /// otherwise stall the whole program for the sake of a caption.
    fn read_repos(&mut self) {
        for dir in self.session.stale_repos(Instant::now()) {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let repo = crate::git::read(&dir);
                let _ = tx.send(Ev::Git(crate::git::Answer { dir, repo }));
            });
        }
    }

    /// Ask `name.rs` about every workspace. Cheap: it is a few string tests
    /// per workspace, and it short-circuits on the first one that fails.
    fn rename_pass(&mut self) {
        let now = Instant::now();
        for p in 0..self.session.projects.len() {
            let repo = self.session.projects[p].name.clone();
            // Without this, a workspace labelled with its own branch name looks
            // hand-written and naming backs off from it permanently.
            let branch = self.session.projects[p]
                .repo
                .as_ref()
                .map(|r| r.branch.clone())
                .unwrap_or_default();
            for w in 0..self.session.projects[p].workspaces.len() {
                let ws = &self.session.projects[p].workspaces[w];
                // The title first, always. The second source is for the pane
                // that has none, and its candidate goes through this same
                // policy with no exemptions.
                let title = ws
                    .active_pane()
                    .and_then(|x| x.title())
                    .or_else(|| ws.suggested.clone());
                let panes = ws.panes.len();
                let current = ws.label.clone();
                let blocked = ws.state == agent::State::Blocked;

                let ws = &mut self.session.projects[p].workspaces[w];
                if !self.cfg.naming.targets.workspace {
                    continue;
                }
                if let name::Decision::Rename(intent) = name::decide(
                    &self.cfg.naming,
                    &mut ws.naming,
                    &current,
                    title.as_deref(),
                    &repo,
                    &branch,
                    panes,
                    blocked,
                    now,
                ) {
                    // The decision is what the intent should be; the template
                    // is how it is arranged. Rendered after, so the policy
                    // compares intents with intents rather than with whatever
                    // a template happened to produce.
                    let mut tokens = self.session.tokens(p, w);
                    tokens.set("intent", intent.clone());
                    tokens.set("intent-slug", name::slugify(&intent, name::AGENT_NAME_MAX));
                    let label = tokens::render(&self.cfg.naming.templates.workspace, &tokens);
                    let ws = &mut self.session.projects[p].workspaces[w];
                    let label = if label.is_empty() { intent } else { label };
                    // Recorded as written, so "did a human change this" compares
                    // the live label against the label rather than against the
                    // intent it was rendered from.
                    ws.naming.applied = Some(label.clone());
                    ws.label = label;
                }
            }
        }
    }

    // ── Geometry ────────────────────────────────────────────────────────

    fn areas(&self, full: Rect) -> (Rect, Rect, Rect) {
        let rail = Rect {
            x: full.x,
            y: full.bottom().saturating_sub(1),
            width: full.width,
            height: 1,
        };
        let body = Rect {
            height: full.height.saturating_sub(1),
            ..full
        };
        let sw = if self.sidebar {
            self.cfg.sidebar_width.min(body.width / 2)
        } else {
            0
        };
        let side = Rect { width: sw, ..body };
        let content = Rect {
            x: body.x + sw,
            width: body.width - sw,
            ..body
        };
        (side, content, rail)
    }

    /// The rects of the panes currently on screen, in the order they are drawn.
    fn visible_rects(&self) -> Vec<Rect> {
        match self.session.focus {
            Focus::Layout(i) => match self.session.layouts.get(i).and_then(|l| l.ws.as_ref()) {
                Some(ws) => ws.rects(self.content).into_iter().map(|(_, r)| r).collect(),
                None => Vec::new(),
            },
            Focus::Ws { p, w } => match self.session.workspace(p, w) {
                Some(ws) => ws.rects(self.content).into_iter().map(|(_, r)| r).collect(),
                None => Vec::new(),
            },
        }
    }

    fn visible_pane_mut(&mut self, index: usize) -> Option<&mut Pane> {
        match self.session.focus {
            Focus::Layout(i) => {
                let ws = self.session.layouts.get_mut(i)?.ws.as_mut()?;
                let id = *ws.tree.leaves().get(index)?;
                ws.pane_mut(id)
            }
            Focus::Ws { p, w } => {
                let ws = self.session.workspace_mut(p, w)?;
                let id = *ws.tree.leaves().get(index)?;
                ws.pane_mut(id)
            }
        }
    }

    // ── Render ──────────────────────────────────────────────────────────

    fn render(&mut self, f: &mut Frame) {
        self.hits.clear();
        let full = f.area();
        let (side, content, rail) = self.areas(full);
        self.content = content;
        self.session.resize_visible(content);

        self.side = side;
        let buf = f.buffer_mut();
        if side.width > 0 {
            self.nav_rows =
                ui::nav::render(buf, side, &self.session, &mut self.nav, &mut self.hits);
        }
        draw_content(buf, content, &self.session, &mut self.hits);

        let clock = chrono::Local::now().format("%H:%M").to_string();
        let status = if self.prefix { "prefix" } else { &self.status };
        ui::rail::render(
            buf,
            rail,
            &self.cfg,
            &self.session,
            &ui::rail::Now {
                clock: &clock,
                note: status,
                quit_armed: self.quit_armed.is_some(),
            },
            &mut self.hits,
        );

        if let Some(p) = &self.picker {
            ui::picker::render(
                buf,
                Rect {
                    height: full.height.saturating_sub(1),
                    ..full
                },
                p,
                &mut self.hits,
            );
            return;
        }

        // The cursor belongs to the focused pane, and only when that pane is
        // showing one.
        // The cursor goes where the screen was drawn, which for a labelled pane
        // is below the rule rather than at the top of its rectangle.
        if let (Some(pane), Some(rect)) = (self.session.active_pane(), self.focused_rect())
            && let Ok(t) = pane.term.lock()
            && let Some(pos) = ui::pane::cursor(
                t.screen(),
                mux::session::content_of(pane.label.is_some(), rect),
            )
        {
            f.set_cursor_position(pos);
        }
    }

    fn focused_rect(&self) -> Option<Rect> {
        let rects = self.visible_rects();
        match self.session.focus {
            Focus::Layout(i) => {
                let ws = self.session.layouts.get(i)?.ws.as_ref()?;
                let at = ws.tree.leaves().iter().position(|&id| id == ws.focus)?;
                rects.get(at).copied()
            }
            Focus::Ws { p, w } => {
                let ws = self.session.workspace(p, w)?;
                let at = ws.tree.leaves().iter().position(|&id| id == ws.focus)?;
                rects.get(at).copied()
            }
        }
    }

    /// Do the thing the user pointed at, however they pointed at it.
    ///
    /// Clicking a row and pressing Enter on it arrive here with the same value,
    /// which is the only reason the two cannot drift apart.
    fn act(&mut self, target: Target) {
        // Doing anything else is a change of mind.
        if !matches!(target, Target::Quit) {
            self.quit_armed = None;
        }
        let area = self.content;
        match target {
            Target::Layout(i) => self.session.open_layout(i, area),
            Target::ProjectFold(p) => {
                if let Some(proj) = self.session.projects.get_mut(p) {
                    proj.expanded = !proj.expanded;
                }
                // Folding is not going anywhere, so the nav keeps the keyboard.
                return;
            }
            Target::Workspace { p, w } => {
                // The rows a keystroke acts on were built for the last frame,
                // and a workspace can close between frames. Focusing one that
                // is gone leaves no focused pane at all.
                if self.session.workspace(p, w).is_none() {
                    return;
                }
                // Clicking a workspace that is already focused and has several
                // panes opens it, which is the only thing left for that click
                // to mean.
                let expand = self.session.focus == Focus::Ws { p, w }
                    && self
                        .session
                        .workspace(p, w)
                        .is_some_and(|ws| ws.panes.len() > 1);
                if expand && let Some(ws) = self.session.workspace_mut(p, w) {
                    ws.expanded = !ws.expanded;
                    return;
                }
                self.session.focus = Focus::Ws { p, w };
            }
            Target::NavPane { p, w, index } => {
                self.session.focus = Focus::Ws { p, w };
                if let Some(ws) = self.session.workspace_mut(p, w)
                    && let Some(&id) = ws.tree.leaves().get(index)
                {
                    ws.focus = id;
                }
            }
            Target::NewWorkspace(p) => {
                self.session.new_workspace(p, area.height, area.width);
            }
            Target::OpenProject => {
                self.open_picker();
                return;
            }
            Target::PickerRow(i) => {
                let path = self
                    .picker
                    .as_ref()
                    .and_then(|p| p.path(i))
                    .map(|p| p.to_path_buf());
                self.picker = None;
                if let Some(path) = path {
                    self.open(&path);
                }
            }
            Target::SortAgents => {
                self.nav.cycle_sort();
                return;
            }
            Target::Attention(state) => {
                // The oldest first: what has been waiting longest is what to
                // look at first. No early return -- this moves you, so it falls
                // through to handing the keyboard back like every other target
                // that does.
                if let Some(at) = self.session.oldest_in(state) {
                    self.session.focus = at;
                }
            }
            Target::Quit => {
                match self.quit_armed {
                    Some(_) => self.quit = true,
                    None => {
                        self.quit_armed = Some(Instant::now());
                        self.note("click again to quit");
                    }
                }
                return;
            }
            Target::Pane { .. } => return,
        }
        // Anything that moved you somewhere hands the keyboard back, because
        // that is what you went there for.
        self.nav.active = false;
    }

    // ── Keys ────────────────────────────────────────────────────────────

    fn on_key(&mut self, k: KeyEvent) {
        if self.picker.is_some() {
            return self.picker_key(k);
        }
        if self.nav.active && !self.prefix {
            return self.nav_key(k);
        }
        if self.prefix {
            self.prefix = false;
            self.status.clear();
            return self.command(k);
        }
        // Ctrl-Space is the prefix. Ctrl-a and Ctrl-b are both load-bearing in
        // every shell line editor; Ctrl-Space is NUL, which nothing sends on
        // purpose.
        if k.code == KeyCode::Char(' ') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.prefix = true;
            self.note("prefix");
            return;
        }
        self.send_key(k);
    }

    /// Keys while the nav holds them. No prefix: a nav that needs one before
    /// every `j` is not a nav.
    fn nav_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.nav.active = false,
            KeyCode::Char('j') | KeyCode::Down => self.nav.step(&self.nav_rows, 1),
            KeyCode::Char('k') | KeyCode::Up => self.nav.step(&self.nav_rows, -1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(t) = self.nav.selection(&self.nav_rows).and_then(Row::target) {
                    self.act(t);
                }
            }
            KeyCode::Char('n') => {
                if let Focus::Ws { p, .. } = self.session.focus {
                    self.act(Target::NewWorkspace(p));
                }
            }
            KeyCode::Char('o') => self.act(Target::OpenProject),
            KeyCode::Char('s') => self.act(Target::SortAgents),
            _ => {}
        }
    }

    fn send_key(&mut self, k: KeyEvent) {
        let Some(pane) = self.session.active_pane_mut() else {
            return;
        };
        if pane.dead {
            return;
        }
        let app_cursor = pane
            .term
            .lock()
            .map(|t| t.screen().application_cursor())
            .unwrap_or(false);
        if let Some(bytes) = keys::encode(k, app_cursor) {
            pane.write(&bytes);
        }
    }

    fn command(&mut self, k: KeyEvent) {
        let (rows, cols) = (self.content.height, self.content.width);
        match k.code {
            // Pressing the prefix twice sends a literal one through.
            KeyCode::Char(' ') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(p) = self.session.active_pane_mut() {
                    p.write(&[0]);
                }
            }
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('n') => {
                if let Focus::Ws { p, .. } = self.session.focus {
                    self.session.new_workspace(p, rows, cols);
                } else {
                    self.note("no project focused");
                }
            }
            KeyCode::Char('o') => self.open_picker(),
            KeyCode::Char('x') => self.session.close_focused(),
            KeyCode::Char('u') => {
                if self.session.release_hold() {
                    self.note("naming released");
                } else {
                    self.note("not held");
                }
            }
            KeyCode::Char('r') => {
                if !self.session.restart_focused(self.content) {
                    self.note("nothing to restart");
                }
            }
            KeyCode::Char('|') | KeyCode::Char('v') => self.session.split(Dir::Cols, rows, cols),
            KeyCode::Char('-') | KeyCode::Char('s') => self.session.split(Dir::Rows, rows, cols),
            KeyCode::Char('d') => self.sidebar = !self.sidebar,
            KeyCode::Char('w') => {
                self.nav.active = !self.nav.active;
                if self.nav.active {
                    // Giving the keyboard to something that is not on screen
                    // looks exactly like a freeze, and the prefix cannot be
                    // reached from inside nav mode to undo it.
                    self.sidebar = true;
                    // Start where the eye already is.
                    self.nav.sync(&self.nav_rows, self.session.focus);
                }
            }
            KeyCode::Char(';') => self.session.cycle_pane(),
            KeyCode::Tab | KeyCode::Char('j') | KeyCode::Down => self.session.step_workspace(1),
            KeyCode::BackTab | KeyCode::Char('k') | KeyCode::Up => self.session.step_workspace(-1),
            KeyCode::Char(c) => {
                if let Some(i) = self
                    .session
                    .layouts
                    .iter()
                    .position(|l| l.def.key == Some(c))
                {
                    self.session.open_layout(i, self.content);
                }
            }
            _ => {}
        }
    }

    // ── Picker ──────────────────────────────────────────────────────────

    fn open_picker(&mut self) {
        self.picker = Some(Picker::new(&self.cfg.projects_root));
    }

    fn picker_key(&mut self, k: KeyEvent) {
        let Some(p) = self.picker.as_mut() else {
            return;
        };
        match k.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Enter => {
                if let Some(i) = p.matches().get(p.selected).map(|(i, _)| *i) {
                    self.act(Target::PickerRow(i));
                }
            }
            KeyCode::Backspace => {
                p.query.pop();
                p.selected = 0;
            }
            KeyCode::Down | KeyCode::Tab => p.move_by(1),
            KeyCode::Up | KeyCode::BackTab => p.move_by(-1),
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                p.query.push(c);
                p.selected = 0;
            }
            _ => {}
        }
    }

    /// Open a project, adding a workspace only if it has none — clicking a
    /// project you already have open should take you there, not pile up shells.
    fn open(&mut self, path: &std::path::Path) {
        let (rows, cols) = (self.content.height, self.content.width);
        let p = self.session.open_project(path);
        match self.session.projects[p].workspaces.is_empty() {
            true => {
                self.session.new_workspace(p, rows, cols);
            }
            false => self.session.focus = Focus::Ws { p, w: 0 },
        }
    }

    // ── Mouse ───────────────────────────────────────────────────────────

    fn on_mouse(&mut self, m: MouseEvent) {
        let target = self.hits.at(m.column, m.row);

        // The divider, before anything else: a drag that started on it owns
        // every event until the button comes up, wherever the pointer has got
        // to by then.
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if self.on_divider(m.column) => {
                self.dragging = true;
                return;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                // Only a floor here. `Ord::clamp` panics when min exceeds max,
                // and on a terminal narrower than forty columns half of it is
                // under the floor -- so a clamp would abort rather than
                // resize. The ceiling is `areas`' job, and it already does it.
                self.cfg.sidebar_width = m.column.max(MIN_NAV);
                return;
            }
            MouseEventKind::Up(_) if self.dragging => {
                self.dragging = false;
                return;
            }
            _ => {}
        }

        // The wheel over the nav scrolls the nav, not whatever pane is behind
        // the pointer.
        if m.column < self.side.right() && self.side.width > 0 {
            let delta: isize = match m.kind {
                MouseEventKind::ScrollDown => 1,
                MouseEventKind::ScrollUp => -1,
                _ => 0,
            };
            if delta != 0 {
                // The section under the pointer, not the nav as a whole.
                self.nav.scroll_by(m.row, delta);
                return;
            }
        }

        if let MouseEventKind::Down(MouseButton::Left) = m.kind
            && let Some(t) = target
            && !matches!(t, Target::Pane { .. })
        {
            return self.act(t);
        }

        // Anything else in the content area belongs to the pane under it.
        if self.picker.is_none() {
            self.send_mouse(&m);
        }
    }

    /// The divider is the nav's last column. Two columns wide as a target,
    /// because one is very hard to hit.
    fn on_divider(&self, column: u16) -> bool {
        self.sidebar
            && self.side.width > 0
            && column + 1 >= self.side.right()
            && column <= self.side.right()
    }

    fn send_mouse(&mut self, m: &MouseEvent) {
        let rects = self.visible_rects();
        let Some(index) = rects.iter().position(|r| {
            m.column >= r.x && m.column < r.right() && m.row >= r.y && m.row < r.bottom()
        }) else {
            return;
        };
        let r = rects[index];

        // Clicking a pane focuses it, whether or not the program inside also
        // wants the click.
        if let MouseEventKind::Down(_) = m.kind
            && let Some(ws) = self.session.focused_workspace_mut()
            && let Some(&id) = ws.tree.leaves().get(index)
        {
            ws.focus = id;
        }

        let Some(pane) = self.visible_pane_mut(index) else {
            return;
        };

        // A labelled pane's terminal starts below the rule, so the event has to
        // be measured from there. Measuring from the rectangle would put every
        // click one row low -- selecting the line above the one you pointed at.
        let inner = mux::session::content_of(pane.label.is_some(), r);
        if m.row < inner.y {
            // The rule itself is chrome. Focusing was the whole of that click.
            return;
        }
        let (col, row) = (m.column - inner.x, m.row - inner.y);

        let bytes = pane
            .term
            .lock()
            .ok()
            .and_then(|t| ui::pane::encode_mouse(t.screen(), m, col, row));
        if let Some(b) = bytes {
            pane.write(&b);
        }
    }
}

/// Draw whatever the focus points at. Free function so the session can be
/// borrowed while the hit map is written.
///
/// A layout and a project workspace draw identically, because a layout is a
/// workspace. The only thing this function decides is which one.
fn draw_content(buf: &mut Buffer, area: Rect, session: &Session, hits: &mut HitMap) {
    let ws = match session.focus {
        Focus::Layout(i) => match session.layouts.get(i).and_then(|l| l.ws.as_ref()) {
            Some(ws) => ws,
            None => return empty(buf, area, "layout is not open"),
        },
        Focus::Ws { p, w } => match session.workspace(p, w) {
            Some(ws) => ws,
            None => return empty(buf, area, "nothing open — press ctrl-space o"),
        },
    };

    for (i, (id, r)) in ws.rects(area).into_iter().enumerate() {
        let Some(pane) = ws.pane(id) else { continue };

        // A labelled pane gives its top row to a rule carrying the label, which
        // is how a five-pane dashboard says which panel is which.
        let inner = mux::session::content_of(pane.label.is_some(), r);
        if let Some(label) = &pane.label {
            rule(
                buf,
                Rect { height: 1, ..r },
                label,
                pane.exit.as_deref(),
                id == ws.focus,
            );
        }
        if let Ok(t) = pane.term.lock() {
            // A stopped pane keeps its last output, dimmed. The output is
            // usually why the panel was there.
            ui::pane::blit(t.screen(), inner, buf, id != ws.focus || pane.dead);
        }
        hits.push(r, Target::Pane { index: i });
    }
}

/// `─ brief ─────────────`. A rule rather than a border: three sides of a box
/// only repeat what the neighbouring pane's own edge already says.
fn rule(buf: &mut Buffer, area: Rect, label: &str, exit: Option<&str>, focused: bool) {
    ui::fill(buf, area, THEME.panel());

    // The tail is measured first: the label's budget has to know about it, or a
    // long label is drawn and then written over halfway through a word.
    let tail = exit.map(|e| format!(" {e} · r restarts "));
    let tail_w = tail.as_ref().map_or(0, |t| {
        (t.chars().count() as u16).min(area.width.saturating_sub(4))
    });

    let mut x = area.x;
    // The focused pane is marked on its own rule. Without it, a dashboard whose
    // panels have all stopped is uniformly dim and nothing says which one `r`
    // would bring back.
    let (lead, lead_style) = if focused {
        ("▊ ", THEME.working())
    } else {
        ("─ ", THEME.rule_strong())
    };
    x += ui::write_str(buf, x, area.y, lead, lead_style, area.width);

    let left = area
        .width
        .saturating_sub(x - area.x)
        .saturating_sub(tail_w + 2) as usize;
    let style = match (exit.is_some(), focused) {
        (false, _) => THEME.title(),
        (true, true) => THEME.text(),
        (true, false) => THEME.dim(),
    };
    x += ui::write_str(buf, x, area.y, &ui::elide(label, left), style, area.width);
    x += ui::write_str(buf, x, area.y, " ", THEME.rule_strong(), area.width);

    for c in x..area.right().saturating_sub(tail_w) {
        ui::write_str(buf, c, area.y, "─", THEME.rule_strong(), 1);
    }
    // Elided rather than dropped: a narrow stopped panel showing frozen output
    // and no explanation is the worst version of this.
    if let Some(t) = tail
        && tail_w > 0
    {
        let text = ui::elide(&t, tail_w as usize);
        ui::write_str(
            buf,
            area.right() - tail_w,
            area.y,
            &text,
            THEME.warn(),
            tail_w,
        );
    }
}

fn empty(buf: &mut Buffer, area: Rect, msg: &str) {
    ratatui::widgets::Clear.render(area, buf);
    if area.height > 1 {
        let x = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
        ui::write_str(
            buf,
            x,
            area.y + area.height / 2,
            msg,
            THEME.faint(),
            area.width,
        );
    }
}
