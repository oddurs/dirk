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
    pub dead: bool,
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
        let program = argv.first().cloned().unwrap_or_default();

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
        cmd.env("DIRK", "1");
        cmd.env("DIRK_PANE", id.to_string());

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
            dead: false,
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

    /// The last OSC title this pane published, if any.
    pub fn title(&self) -> Option<String> {
        self.term
            .lock()
            .ok()
            .and_then(|t| t.callbacks().title.clone())
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.dead = true;
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        if !self.dead {
            let _ = self.child.kill();
        }
    }
}
