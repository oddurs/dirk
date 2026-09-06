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

mod config;
mod hit;
mod keys;
mod mux;
mod name;
mod theme;
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
use ui::picker::Picker;

const USAGE: &str = "\
Usage: dirk [OPTION]...
Run a terminal multiplexer with a clickable project tree, static pages, and
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

    let session = Session::new(&cfg, tx);
    let mut app = App::new(cfg, session, size);
    app.bootstrap();

    let result = app.run(&mut terminal, rx);
    restore();
    result
}

// ── Terminal lifecycle ──────────────────────────────────────────────────

type Term = Terminal<CrosstermBackend<io::Stdout>>;

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
    quit: bool,
}

impl App {
    fn new(cfg: Config, session: Session, size: ratatui::layout::Size) -> Self {
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
            Ev::Output(_) => self.rename_pass(),
            Ev::Exited(id) => {
                self.session.reap(id);
                self.session.refocus();
            }
            Ev::Tick => {
                self.rename_pass();
                if !self.status.is_empty()
                    && self.status_at.elapsed() > Duration::from_secs(3)
                    && !self.prefix
                {
                    self.status.clear();
                }
            }
        }
    }

    /// Ask `name.rs` about every workspace. Cheap: it is a few string tests
    /// per workspace, and it short-circuits on the first one that fails.
    fn rename_pass(&mut self) {
        let now = Instant::now();
        for p in 0..self.session.projects.len() {
            let repo = self.session.projects[p].name.clone();
            for w in 0..self.session.projects[p].workspaces.len() {
                let ws = &self.session.projects[p].workspaces[w];
                let title = ws.active_pane().and_then(|x| x.title());
                let panes = ws.panes.len();
                let current = ws.label.clone();

                let ws = &mut self.session.projects[p].workspaces[w];
                if let name::Decision::Rename(label) = name::decide(
                    &self.cfg.naming,
                    &mut ws.naming,
                    &current,
                    title.as_deref(),
                    &repo,
                    "",
                    panes,
                    now,
                ) {
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
            Focus::Page(i) => match self.session.pages.get(i).and_then(|p| p.pane.as_ref()) {
                Some(_) => vec![self.content],
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
            Focus::Page(i) => self.session.pages.get_mut(i)?.pane.as_mut(),
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

        let buf = f.buffer_mut();
        if side.width > 0 {
            ui::sidebar::render(buf, side, &self.session, &mut self.hits);
        }
        draw_content(buf, content, &self.session, &mut self.hits);

        let clock = chrono::Local::now().format("%H:%M").to_string();
        let status = if self.prefix { "prefix" } else { &self.status };
        ui::rail::render(
            buf,
            rail,
            &self.cfg,
            &self.session,
            &clock,
            status,
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
        if let (Some(pane), Some(rect)) = (self.session.active_pane(), self.focused_rect())
            && let Ok(t) = pane.term.lock()
            && let Some(pos) = ui::pane::cursor(t.screen(), rect)
        {
            f.set_cursor_position(pos);
        }
    }

    fn focused_rect(&self) -> Option<Rect> {
        let rects = self.visible_rects();
        match self.session.focus {
            Focus::Page(_) => rects.first().copied(),
            Focus::Ws { p, w } => {
                let ws = self.session.workspace(p, w)?;
                let at = ws.tree.leaves().iter().position(|&id| id == ws.focus)?;
                rects.get(at).copied()
            }
        }
    }

    // ── Keys ────────────────────────────────────────────────────────────

    fn on_key(&mut self, k: KeyEvent) {
        if self.picker.is_some() {
            return self.picker_key(k);
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

    fn send_key(&mut self, k: KeyEvent) {
        let Some(pane) = self.session.active_pane_mut() else {
            return;
        };
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
            KeyCode::Char('|') | KeyCode::Char('v') => self.session.split(Dir::Cols, rows, cols),
            KeyCode::Char('-') | KeyCode::Char('s') => self.session.split(Dir::Rows, rows, cols),
            KeyCode::Char('d') => self.sidebar = !self.sidebar,
            KeyCode::Char(';') => self.session.cycle_pane(),
            KeyCode::Tab | KeyCode::Char('j') | KeyCode::Down => self.session.step_workspace(1),
            KeyCode::BackTab | KeyCode::Char('k') | KeyCode::Up => self.session.step_workspace(-1),
            KeyCode::Char(c) => {
                if let Some(i) = self.session.pages.iter().position(|p| p.def.key == Some(c)) {
                    self.session.open_page(i, rows, cols);
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
                let chosen = p.matches().get(p.selected).map(|(i, _)| *i);
                if let Some(i) = chosen
                    && let Some(path) = p.path(i).map(|x| x.to_path_buf())
                {
                    self.picker = None;
                    self.open(&path);
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
        let (rows, cols) = (self.content.height, self.content.width);

        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
            match target {
                Some(Target::Page(i)) => return self.session.open_page(i, rows, cols),
                Some(Target::ProjectFold(p)) => {
                    if let Some(proj) = self.session.projects.get_mut(p) {
                        proj.expanded = !proj.expanded;
                    }
                    return;
                }
                Some(Target::Workspace { p, w }) => {
                    self.session.focus = Focus::Ws { p, w };
                    return;
                }
                Some(Target::NewWorkspace(p)) => {
                    self.session.new_workspace(p, rows, cols);
                    return;
                }
                Some(Target::OpenProject) => return self.open_picker(),
                Some(Target::PickerRow(i)) => {
                    let path = self
                        .picker
                        .as_ref()
                        .and_then(|p| p.path(i))
                        .map(|p| p.to_path_buf());
                    self.picker = None;
                    if let Some(path) = path {
                        self.open(&path);
                    }
                    return;
                }
                _ => {}
            }
        }

        // Anything else in the content area belongs to the pane under it.
        if self.picker.is_none() {
            self.send_mouse(&m);
        }
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

        let (col, row) = (m.column - r.x, m.row - r.y);
        let Some(pane) = self.visible_pane_mut(index) else {
            return;
        };
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
fn draw_content(buf: &mut Buffer, area: Rect, session: &Session, hits: &mut HitMap) {
    let rects = match session.focus {
        Focus::Page(i) => match session.pages.get(i).and_then(|p| p.pane.as_ref()) {
            Some(pane) => {
                if let Ok(t) = pane.term.lock() {
                    ui::pane::blit(t.screen(), area, buf, false);
                }
                hits.push(area, Target::Pane { index: 0 });
                return;
            }
            None => return empty(buf, area, "page is not running"),
        },
        Focus::Ws { p, w } => {
            let Some(ws) = session.workspace(p, w) else {
                return empty(buf, area, "nothing open — press ctrl-space o");
            };
            let placed = ws.rects(area);
            for (i, (id, r)) in placed.iter().enumerate() {
                if let Some(pane) = ws.pane(*id)
                    && let Ok(t) = pane.term.lock()
                {
                    ui::pane::blit(t.screen(), *r, buf, *id != ws.focus);
                }
                hits.push(*r, Target::Pane { index: i });
            }
            placed.into_iter().map(|(_, r)| r).collect::<Vec<_>>()
        }
    };
    let _ = rects;
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
