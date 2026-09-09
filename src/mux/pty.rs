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

//! One pane: a PTY, the process inside it, and the terminal state it drew.
//!
//! Everything hard about a multiplexer lives behind `vt100`. This file owns
//! only the plumbing either side of it — spawn the child on a pty, pump its
//! output into a parser on a thread, and let the UI thread read the resulting
//! grid. The parser is behind a mutex because exactly two threads touch it: the
//! reader writes, the renderer reads.
//!
//! The parser carries a [`Sink`] so OSC title sequences are captured rather
//! than discarded. That is not a detail — an agent's own summary of what it is
//! doing arrives that way, and it is the entire input to `name.rs`. The same
//! sink collects the answers a program is owed: a terminal is asked questions
//! as well as told things, and one that never answers is one every shell waits
//! on before it prints a prompt.

use crate::mux::Ev;
use portable_pty::{CommandBuilder, PtyPair, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub type PaneId = u64;

/// What the parser hands back: the title a program set, and the answers it
/// is owed.
///
/// vt100 drops window titles on the floor unless a callback claims them, and
/// for a long time that was the only reason dirk installed callbacks at all.
/// It also drops every question a program asks its terminal, and a program
/// that asks one waits for the reply: fish 4 sits for ten seconds on an
/// unanswered "who are you" before it prints its first prompt.
#[derive(Default)]
pub struct Sink {
    /// `ESC ] 2 ; <title> BEL`, the most recent.
    pub title: Option<String>,
    /// Replies owed to the program, in the order it asked. The reader thread
    /// takes them after every chunk and hands them to the loop, which is the
    /// one thing that writes to a pane.
    pub answers: Vec<u8>,
}

/// Who a pane's terminal says it is: a VT220 with colour, which is about what
/// vt100 emulates. Not the xterm answer, whatever `TERM` claims — a program
/// that believes it is talking to xterm starts using things the parser has
/// never heard of.
const DA1: &[u8] = b"\x1b[?62;22c";

/// The secondary attributes. The first number is the terminal's own letter,
/// the way tmux answers 84 for `T`, and the version is zero. Nothing reads
/// more than the shape of this, and vim in particular reads a small version as
/// "not xterm", which is right.
const DA2: &[u8] = b"\x1b[>100;0;0c";

/// How much of a reply is owed per chunk of output.
///
/// A program that asks a thousand questions in one write is not waiting for
/// any of them, and a pane that answered every one would have that much to
/// write into a pty nothing is reading from.
const OWED: usize = 256;

impl vt100::Callbacks for Sink {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        let t = String::from_utf8_lossy(title).trim().to_string();
        self.title = if t.is_empty() { None } else { Some(t) };
    }

    /// The questions a program asks its terminal, and the answers.
    ///
    /// Only what dirk can stand behind. A query for a protocol the pane does
    /// not speak — the kitty keyboard protocol's `CSI ? u`, a `DECRQM` for a
    /// mode vt100 does not track — gets nothing, because silence is how those
    /// protocols say "unsupported". That is safe precisely because DA1 is
    /// answered: every program that probes sends DA1 last and stops waiting
    /// when it comes back.
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if self.answers.len() >= OWED {
            return;
        }
        let first = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        let (row, col) = screen.cursor_position();
        let (row, col) = (u32::from(row) + 1, u32::from(col) + 1);
        match (i1, c, first) {
            (None, 'c', 0) => self.answers.extend_from_slice(DA1),
            (Some(b'>'), 'c', 0) => self.answers.extend_from_slice(DA2),
            // DSR: "are you all right", and "where is the cursor", one-based.
            (None, 'n', 5) => self.answers.extend_from_slice(b"\x1b[0n"),
            (None, 'n', 6) => self
                .answers
                .extend_from_slice(format!("\x1b[{row};{col}R").as_bytes()),
            (Some(b'?'), 'n', 6) => self
                .answers
                .extend_from_slice(format!("\x1b[?{row};{col};1R").as_bytes()),
            // XTVERSION, which fish reads to decide which terminal it is
            // working around. Named for what it is, so a workaround for
            // something else is not applied here.
            (Some(b'>'), 'q', 0) => self.answers.extend_from_slice(
                concat!("\x1bP>|dirk ", env!("CARGO_PKG_VERSION"), "\x1b\\").as_bytes(),
            ),
            _ => {}
        }
    }
}

pub type Term = vt100::Parser<Sink>;

/// Whether the pty would take one byte now, without waiting for it.
///
/// One byte is all `POLLOUT` promises. macOS reports a master writable with
/// a single byte of room in the line's queue, and a blocking write of more
/// than that room sits until something reads; Linux promises 256. Either way
/// the answer is per byte, and so is the write that follows it.
fn writable(fd: std::os::fd::RawFd) -> bool {
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    // The pollfd outlives the call and the count matches it.
    let ready = unsafe { libc::poll(&mut p, 1, 0) };
    ready > 0 && p.revents & libc::POLLOUT != 0
}

pub struct Pane {
    pub id: PaneId,
    pub term: Arc<Mutex<Term>>,
    pub cwd: PathBuf,
    /// Kept so a stopped pane can be started again in place.
    pub argv: Vec<String>,
    /// Drawn as a rule above the pane. Set for panes that came from a layout,
    /// where knowing which panel is which is most of the point; a shell you
    /// opened yourself needs no caption.
    pub label: Option<String>,
    pub dead: bool,
    /// How the program ended, once it has. Shown in the pane's rule, because a
    /// panel that stopped should say so rather than simply going quiet.
    pub exit: Option<String>,
    /// Set when a human closed this pane, as opposed to its program exiting on
    /// its own. The two look identical from the pty and mean opposite things.
    pub closing: bool,
    /// What is running here, as of the last sample.
    pub occupant: crate::agent::Occupant,
    /// The images this pane's program has drawn, and where.
    ///
    /// Beside the grid rather than in it: vt100 discards a graphics APC without
    /// a word — no callback, nothing left in the cells — so the bytes are taken
    /// out of the stream before it sees them and kept here. Shared with the
    /// reader thread, which is the only place the bytes exist.
    pub graphics: Arc<Mutex<crate::graphics::Store>>,
    /// What programs outside dirk have said about this pane, for display.
    ///
    /// Deliberately not state. `agent state` is a small closed set that dirk
    /// reasons about — it drives waits, notifications, ordering and the
    /// attention column — and it has to stay that way. Everything a program
    /// wants to *show* had nowhere to go except into a state it would then be
    /// reasoned about with, which is how an indexer's progress ends up
    /// interrupting somebody.
    ///
    /// Ephemeral. These describe a moment in a process that is gone after a
    /// restart, and restoring them would be restoring a claim nobody is making
    /// any more.
    pub metadata: std::collections::BTreeMap<String, String>,
    /// The wrapper that named the harness in here, when one did.
    ///
    /// Set when `DIRK_AGENT` on an unrecognised foreground process decided what
    /// this pane holds. Kept so `agent explain` can say that a hint decided it
    /// rather than leaving somebody to wonder how dirk recognised `fence`.
    pub hinted: Option<String>,
    /// What to call the agent in here, when there is one.
    ///
    /// Unique among live agents, because a name is how one is addressed. It
    /// follows the agent rather than the pane, and goes when it does.
    pub agent_name: Option<String>,
    /// How far back in the scrollback this pane is being read.
    ///
    /// Zero is the live screen. Kept here rather than only in vt100 because it
    /// decides whether the pane says it is not at the bottom, and because it
    /// has to be put back to zero when new output arrives -- a pane you scrolled
    /// away from and then typed into should show you what you typed.
    pub scroll: usize,
    /// When this pane last produced output.
    ///
    /// Per pane, not per workspace: a workspace holding an agent beside a
    /// `npm run dev` would otherwise look busy for ever, because the server's
    /// output would keep the whole workspace fresh and the agent could never be
    /// seen to stop.
    pub touched: std::time::Instant,
    writer: Box<dyn Write + Send>,
    /// The pty, whether this process opened it or inherited it across a
    /// handoff. Panes from before the binary changed are otherwise identical.
    pub(crate) master: super::adopt::Master,
    child: super::adopt::Kid,
    rows: u16,
    cols: u16,
}

/// What a pane is made of, once the pty and the process exist.
///
/// A struct because `assemble` is reached from two directions -- spawning and
/// adopting -- and six positional arguments in two places is five chances to
/// put two of them the wrong way round.
struct Made {
    id: PaneId,
    cwd: std::path::PathBuf,
    argv: Vec<String>,
    rows: u16,
    cols: u16,
    scrollback: usize,
}

/// Everything about starting a pane that is not what to run or where.
///
/// A struct rather than four more arguments: the four travel together through
/// every call site, and three of them are numbers that would sit next to each
/// other waiting to be swapped.
#[derive(Debug, Clone, Copy)]
pub struct Setup {
    pub rows: u16,
    pub cols: u16,
    pub scrollback: usize,
    /// Make an interactive shell a login shell, which is what reads the files
    /// that build a login `PATH`. Only meaningful when the argv is a shell: a
    /// board's command is a command and is run as one.
    pub login: bool,
}

/// Which line of the pane's whole history the cursor is on, and its column.
///
/// vt100 does not say how much has scrolled off, but it clamps `set_scrollback`
/// to what exists — so asking for more than there could be and reading back
/// gives the total. The view is put straight back, under the same lock, so
/// nothing draws in between.
pub fn whole_history(term: &mut Term) -> (usize, u16) {
    let was = term.screen().scrollback();
    term.screen_mut().set_scrollback(usize::MAX / 2);
    let scrolled = term.screen().scrollback();
    term.screen_mut().set_scrollback(was);
    let (row, col) = term.screen().cursor_position();
    (scrolled + usize::from(row), col)
}

/// The line of the pane's whole history the top of the visible screen is on.
///
/// The inverse of [`whole_history`]: what a placement's line has to be measured
/// against to find the row it is drawn on now.
pub fn top_line(term: &mut Term) -> usize {
    let was = term.screen().scrollback();
    term.screen_mut().set_scrollback(usize::MAX / 2);
    let scrolled = term.screen().scrollback();
    term.screen_mut().set_scrollback(was);
    scrolled.saturating_sub(was)
}

fn oops(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

impl Pane {
    pub fn spawn(
        id: PaneId,
        argv: &[String],
        cwd: &Path,
        how: Setup,
        tx: Sender<Ev>,
    ) -> std::io::Result<Self> {
        let Setup {
            rows,
            cols,
            scrollback,
            login,
        } = how;
        let (rows, cols) = (rows.max(1), cols.max(1));
        // Guarded rather than assumed: `argv[1..]` on an empty slice panics, and
        // a layout pane may legally be written with a title and no command.
        let Some(program) = argv.first().cloned() else {
            return Err(std::io::Error::other("no command to run"));
        };

        let PtyPair { slave, master } = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(oops)?;

        let mut cmd = CommandBuilder::new(&program);
        // `-l` rather than an argv[0] beginning with a dash, which is the older
        // convention and the one this pty library reserves for the shell it
        // picks itself. Every shell somebody would set here takes `-l`; one
        // that does not wants `shell_mode = "non_login"`, and says so in the
        // manual.
        if login {
            cmd.arg("-l");
        }
        for a in &argv[1..] {
            cmd.arg(a);
        }
        cmd.cwd(cwd);
        // Claim 256 colours and truecolor so programs do not fall back to a
        // sixteen-colour palette against a theme that assumes rgb.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // So a shell prompt or an agent can tell it is inside dirk.
        // So a shell prompt, or an agent, can tell it is inside dirk and say
        // which pane it is in — which is what `--current` resolves from.
        cmd.env("DIRK", "1");
        cmd.env("DIRK_PANE_ID", format!("p{id}"));
        if let Ok(session) = std::env::var("DIRK_SESSION") {
            cmd.env("DIRK_SESSION", session);
        }

        let child = super::adopt::Kid::Owned(slave.spawn_command(cmd).map_err(oops)?);
        // The slave fd must go, or the pty never reports EOF when the child
        // exits and the reader thread blocks for the life of the process.
        drop(slave);

        Self::assemble(
            super::adopt::Master::Owned(master),
            child,
            Made {
                id,
                cwd: cwd.to_path_buf(),
                argv: argv.to_vec(),
                rows,
                cols,
                scrollback,
            },
            tx,
        )
    }

    /// A pane whose pty and process were already running when this image
    /// started, handed over by the one it replaced.
    ///
    /// Everything below the descriptor is identical: the same reader thread,
    /// the same terminal, the same events. What differs is only that nothing
    /// here opened the pty or started the program.
    pub fn adopt(
        note: &crate::handoff::Pane,
        scrollback: usize,
        tx: Sender<Ev>,
    ) -> std::io::Result<Self> {
        if !super::adopt::alive(note.fd) {
            return Err(std::io::Error::other(format!(
                "pane {}: descriptor {} did not survive",
                note.id, note.fd
            )));
        }
        let mut pane = Self::assemble(
            super::adopt::Master::Adopted(super::adopt::Adopted::new(note.fd)),
            super::adopt::Kid::Inherited(super::adopt::Inherited::new(note.pid)),
            Made {
                id: note.id,
                cwd: note.cwd.clone(),
                argv: note.argv.clone(),
                rows: note.rows,
                cols: note.cols,
                scrollback,
            },
            tx,
        )?;
        pane.label = note.label.clone();
        pane.agent_name = note.agent_name.clone();
        // The modes before the screen: a screen drawn while the parser still
        // thinks the cursor is hidden ends with it hidden, and the sequence
        // that would have shown it went past before this was written down.
        if let Ok(mut term) = pane.term.lock() {
            term.process(&note.modes.sequences());
            term.process(&crate::handoff::unbase64(&note.screen));
            // The replay is what the screen looked like, not what was said to
            // it. Nothing in it is a question, and the image that heard the
            // questions answered them.
            term.callbacks_mut().answers.clear();
        }
        Ok(pane)
    }

    /// Everything a pane is once there is a pty and a process behind it.
    fn assemble(
        master: super::adopt::Master,
        child: super::adopt::Kid,
        made: Made,
        tx: Sender<Ev>,
    ) -> std::io::Result<Self> {
        let Made {
            id,
            cwd,
            argv,
            rows,
            cols,
            scrollback,
        } = made;
        let mut reader = master.try_clone_reader()?;
        let writer = master.take_writer()?;

        let term = Arc::new(Mutex::new(Term::new_with_callbacks(
            rows,
            cols,
            scrollback,
            Sink::default(),
        )));

        let sink = Arc::clone(&term);
        let images = Arc::new(Mutex::new(crate::graphics::Store::default()));
        let drawing = Arc::clone(&images);
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            let mut apc = crate::graphics::Reader::default();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(Ev::Exited(id));
                        return;
                    }
                    Ok(n) => {
                        // The graphics come out first: vt100 would swallow them
                        // and there is no way to ask it what it swallowed.
                        let (text, drawn) = apc.take(&buf[..n]);
                        let mut owed = Vec::new();
                        if let Ok(mut t) = sink.lock() {
                            t.process(&text);
                            owed = std::mem::take(&mut t.callbacks_mut().answers);
                            if !drawn.is_empty()
                                && let Ok(mut store) = drawing.lock()
                            {
                                // Anchored to the line of the pane's whole
                                // history the cursor is on, so the image moves
                                // with its text rather than with the screen.
                                let (line, col) = whole_history(&mut t);
                                for cmd in &drawn {
                                    store.apply(cmd, line, col);
                                }
                            }
                        }
                        // A send failure means the UI is gone; so is the reason
                        // to keep reading.
                        if !owed.is_empty() && tx.send(Ev::Answer(id, owed)).is_err() {
                            return;
                        }
                        if tx.send(Ev::Output(id)).is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Ok(Self {
            id,
            term,
            cwd,
            argv,
            label: None,
            dead: false,
            exit: None,
            closing: false,
            occupant: crate::agent::Occupant::default(),
            graphics: images,
            metadata: std::collections::BTreeMap::new(),
            hinted: None,
            agent_name: None,
            scroll: 0,
            touched: std::time::Instant::now(),
            writer,
            master,
            child,
            rows,
            cols,
        })
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.max(1), cols.max(1));
        if (rows, cols) == (self.rows, self.cols) {
            return;
        }
        self.rows = rows;
        self.cols = cols;
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut t) = self.term.lock() {
            t.screen_mut().set_size(rows, cols);
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// Answer something the program asked, if it is still listening.
    ///
    /// `write` blocks once the pty's input queue is full, and a program that
    /// floods its pane with questions and never reads is exactly the one that
    /// fills it — from there the loop would hang on a pane that had stopped
    /// listening. A terminal's driver drops input at that point, and so does
    /// this: each byte is written only after the pty has said it will take
    /// one, and the rest of the reply is dropped the moment it will not. A
    /// reply cut short is garbage to the program, but a program whose queue
    /// is full is not reading it, and one that starts again gets its next
    /// question answered whole.
    pub fn answer(&mut self, bytes: &[u8]) {
        let Some(fd) = self.master.raw_fd() else {
            return;
        };
        for b in bytes {
            if !writable(fd) || self.writer.write_all(std::slice::from_ref(b)).is_err() {
                break;
            }
        }
        let _ = self.writer.flush();
    }

    /// The process group the tty currently has in the foreground.
    ///
    /// This is what a shell sets before it waits for a command and resets when
    /// the command ends, so it is exactly "what is running in this pane right
    /// now" — not what was started in it, and not what its children are up to.
    #[cfg(unix)]
    pub fn foreground(&self) -> Option<i32> {
        if self.dead {
            return None;
        }
        self.master.process_group_leader()
    }

    #[cfg(not(unix))]
    pub fn foreground(&self) -> Option<i32> {
        None
    }

    /// The last OSC title this pane published, if any.
    pub fn title(&self) -> Option<String> {
        self.term
            .lock()
            .ok()
            .and_then(|t| t.callbacks().title.clone())
    }

    /// The process behind this pane, for writing down before a handoff.
    pub fn pid(&self) -> Option<i32> {
        self.child.pid()
    }

    /// Can this pane's grid still be read?
    ///
    /// A `Mutex` fails to lock for exactly one reason: a thread panicked while
    /// holding it. For `term` that thread is this pane's reader, so the grid is
    /// frozen at whatever it held and nothing will ever add to it again.
    ///
    /// Every place that reads it handles the failure the same way — do nothing
    /// this time — which is right at each of the twenty-five of them and adds up
    /// to something wrong: a pane that stays on screen, keeps its row and its
    /// place in the attention column, and says nothing at all about why it
    /// stopped. It looks exactly like a program that has gone quiet.
    pub fn unreadable(&self) -> bool {
        self.term.is_poisoned()
    }

    /// The grid can no longer be read, so this pane is over.
    ///
    /// Ended rather than merely marked. Nothing it writes from here on can
    /// reach a screen, so leaving the program running leaves it talking into a
    /// closed pipe — the same argument `finish` makes about a pty whose other
    /// end has gone, and the same treatment.
    pub fn give_up(&mut self) {
        self.finish();
        // Said plainly, because this is not how a program usually ends and the
        // difference is the whole of what somebody needs in order to work out
        // what happened.
        self.exit = Some("stopped: dirk could no longer read it".into());
    }

    /// Close this pane for good. The child's exit arrives as an event like any
    /// other; `closing` is what tells that event this was deliberate.
    pub fn close(&mut self) {
        self.closing = true;
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.dead = true;
    }

    /// Note that the program has ended, and how.
    ///
    /// Never blocks. EOF on the pty means the last slave descriptor closed, not
    /// that the process is gone — a program that daemonizes, or a read that
    /// failed for its own reasons, gets here with the child still running. A
    /// blocking `wait` on the drawing thread would then hang the whole program
    /// with no redraws and no keys, which looks exactly like a freeze.
    pub fn finish(&mut self) {
        self.dead = true;
        let mut status = self.child.try_wait().ok().flatten();
        if status.is_none() {
            // Still running with nothing on the other end of the pty. Nothing
            // can reach it and it can reach nothing; end it.
            let _ = self.child.kill();
            status = self.child.try_wait().ok().flatten();
        }
        self.exit = Some(match status {
            Some(s) if s.success() => "exited".into(),
            Some(s) => format!("exited {}", s.exit_code()),
            None => "stopped".into(),
        });
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        if !self.dead {
            let _ = self.child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a program that wrote `bytes` to its pane would be answered.
    fn asked(bytes: &[u8]) -> Vec<u8> {
        let mut term = Term::new_with_callbacks(24, 80, 0, Sink::default());
        term.process(bytes);
        std::mem::take(&mut term.callbacks_mut().answers)
    }

    #[test]
    fn who_are_you_is_answered_with_what_vt100_is() {
        assert_eq!(asked(b"\x1b[c"), DA1);
        assert_eq!(asked(b"\x1b[0c"), DA1);
        assert_eq!(asked(b"\x1b[>c"), DA2);
        assert_eq!(asked(b"\x1b[>0c"), DA2);
    }

    #[test]
    fn the_cursor_is_reported_one_based() {
        assert_eq!(asked(b"abc\x1b[6n"), b"\x1b[1;4R");
        assert_eq!(asked(b"\r\n\x1b[?6n"), b"\x1b[?2;1;1R");
        assert_eq!(asked(b"\x1b[5n"), b"\x1b[0n");
    }

    #[test]
    fn the_version_is_the_one_cargo_built() {
        let reply = asked(b"\x1b[>q");
        let text = String::from_utf8(reply).unwrap();
        assert_eq!(
            text,
            format!("\x1bP>|dirk {}\x1b\\", env!("CARGO_PKG_VERSION"))
        );
    }

    /// What fish 4 sends at startup: a kitty keyboard query, then DA1 as the
    /// terminator. The first is not answered, because saying nothing is how a
    /// terminal declines that protocol; the second is, or fish waits ten
    /// seconds.
    #[test]
    fn a_protocol_dirk_does_not_speak_gets_silence_and_da1_still_comes_back() {
        assert_eq!(asked(b"\x1b[?u\x1b[c"), DA1);
        assert!(asked(b"\x1b[?2026$p\x1b[=c").is_empty());
    }

    #[test]
    fn answers_come_back_in_the_order_asked() {
        let mut want = b"\x1b[1;1R".to_vec();
        want.extend_from_slice(DA1);
        assert_eq!(asked(b"\x1b[6n\x1b[c"), want);
    }

    #[test]
    fn a_flood_of_questions_is_answered_only_so_far() {
        let flood = b"\x1b[c".repeat(1000);
        let owed = asked(&flood);
        assert!(!owed.is_empty());
        assert!(owed.len() < OWED + DA1.len(), "{}", owed.len());
    }

    /// A pane running something that will sit still, and its grid.
    fn a_pane() -> (Pane, Arc<Mutex<Term>>) {
        let (tx, _rx) = std::sync::mpsc::channel();
        let pane = Pane::spawn(
            1,
            &["sleep".into(), "30".into()],
            Path::new("."),
            Setup {
                rows: 24,
                cols: 80,
                scrollback: 100,
                login: false,
            },
            tx,
        )
        .expect("spawn");
        let grid = Arc::clone(&pane.term);
        (pane, grid)
    }

    #[test]
    fn a_pane_whose_grid_cannot_be_read_is_a_pane_that_has_ended() {
        let (mut pane, grid) = a_pane();
        assert!(!pane.unreadable(), "a working pane reads as broken");

        // The one thing that poisons a lock, done deliberately. The hook is
        // swapped out first because the panic is the point of the test and its
        // backtrace is not something anybody reading the output wants.
        let was = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let _ = std::thread::spawn(move || {
            let _held = grid.lock().unwrap();
            panic!("the reader fell over");
        })
        .join();
        std::panic::set_hook(was);

        assert!(pane.unreadable(), "a poisoned grid did not read as broken");
        // Before: alive, silent, and indistinguishable from an idle agent.
        assert!(!pane.dead);

        pane.give_up();
        assert!(pane.dead, "the pane was left alive with no way to read it");
        let said = pane.exit.as_deref().unwrap_or_default();
        assert!(
            said.contains("could no longer read"),
            "it ended without saying why: {said:?}"
        );
    }

    #[test]
    fn an_ordinary_pane_is_left_alone() {
        // The sweep runs on every turn of the event loop, so it has to be sure
        // about the panes it does not touch as well as the ones it does.
        let (pane, _grid) = a_pane();
        assert!(!pane.unreadable());
        assert!(!pane.dead);
    }
}
