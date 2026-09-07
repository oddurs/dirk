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
//! The parser carries a [`TitleSink`] so OSC title sequences are captured
//! rather than discarded. That is not a detail — an agent's own summary of what
//! it is doing arrives that way, and it is the entire input to `name.rs`.

use crate::mux::Ev;
use portable_pty::{Child, CommandBuilder, MasterPty, PtyPair, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub type PaneId = u64;

/// Captures `ESC ] 2 ; <title> BEL`. vt100 drops window titles on the floor
/// unless a callback claims them, and this is the only reason dirk installs
/// callbacks at all.
#[derive(Default)]
pub struct TitleSink {
    pub title: Option<String>,
}

impl vt100::Callbacks for TitleSink {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        let t = String::from_utf8_lossy(title).trim().to_string();
        self.title = if t.is_empty() { None } else { Some(t) };
    }
}

pub type Term = vt100::Parser<TitleSink>;

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
    /// What to call the agent in here, when there is one.
    ///
    /// Unique among live agents, because a name is how one is addressed. It
    /// follows the agent rather than the pane, and goes when it does.
    pub agent_name: Option<String>,
    /// When this pane last produced output.
    ///
    /// Per pane, not per workspace: a workspace holding an agent beside a
    /// `npm run dev` would otherwise look busy for ever, because the server's
    /// output would keep the whole workspace fresh and the agent could never be
    /// seen to stop.
    pub touched: std::time::Instant,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    rows: u16,
    cols: u16,
}

fn oops(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

impl Pane {
    pub fn spawn(
        id: PaneId,
        argv: &[String],
        cwd: &Path,
        rows: u16,
        cols: u16,
        scrollback: usize,
        tx: Sender<Ev>,
    ) -> std::io::Result<Self> {
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

        let child = slave.spawn_command(cmd).map_err(oops)?;
        // The slave fd must go, or the pty never reports EOF when the child
        // exits and the reader thread blocks for the life of the process.
        drop(slave);

        let mut reader = master.try_clone_reader().map_err(oops)?;
        let writer = master.take_writer().map_err(oops)?;

        let term = Arc::new(Mutex::new(Term::new_with_callbacks(
            rows,
            cols,
            scrollback,
            TitleSink::default(),
        )));

        let sink = Arc::clone(&term);
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(Ev::Exited(id));
                        return;
                    }
                    Ok(n) => {
                        if let Ok(mut t) = sink.lock() {
                            t.process(&buf[..n]);
                        }
                        // A send failure means the UI is gone; so is the reason
                        // to keep reading.
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
            cwd: cwd.to_path_buf(),
            argv: argv.to_vec(),
            label: None,
            dead: false,
            exit: None,
            closing: false,
            occupant: crate::agent::Occupant::default(),
            agent_name: None,
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
