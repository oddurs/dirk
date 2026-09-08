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

//! Panes that were already running when this process started.
//!
//! A handoff replaces the binary without stopping the work: the server clears
//! `FD_CLOEXEC` on every pty it holds, writes down which descriptor belongs to
//! which pane, and `exec`s the new binary over itself. Same process, so the
//! shells and agents keep the parent they had; same descriptors, so the ptys
//! they are talking through are the ones they were already talking through.
//!
//! What arrives on the other side is a number, not a `MasterPty`. This is the
//! small amount of pty that dirk actually uses, done directly on the
//! descriptor: resize, a reader, a writer, and which process group is in the
//! foreground. Implementing `portable_pty::MasterPty` instead would mean taking
//! `downcast-rs` and `nix` as direct dependencies to satisfy a trait bound, for
//! four methods dirk already calls by name.
//!
//! The child is the same story. `exec` keeps our children, so `waitpid` still
//! answers for them -- but the handle that knew how to ask went with the old
//! image, and a pid is enough to rebuild it.

use portable_pty::{Child, ExitStatus, MasterPty, PtySize};
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};

/// A pty we inherited: everything dirk asks of a master, on a descriptor.
pub struct Adopted(RawFd);

impl Adopted {
    pub fn new(fd: RawFd) -> Self {
        Adopted(fd)
    }

    /// A separate open file description on the same pty.
    ///
    /// `dup` rather than sharing: the reader runs on its own thread and the
    /// writer on the loop's, and a single `File` would have them contending
    /// for one handle's cursor and closing it out from under each other.
    fn duplicate(&self) -> std::io::Result<std::fs::File> {
        let fd = unsafe { libc::dup(self.0) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { std::fs::File::from_raw_fd(fd) })
    }

    fn resize(&self, size: PtySize) -> std::io::Result<()> {
        let winsize = libc::winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: size.pixel_width,
            ws_ypixel: size.pixel_height,
        };
        match unsafe { libc::ioctl(self.0, libc::TIOCSWINSZ, &winsize) } {
            0 => Ok(()),
            _ => Err(std::io::Error::last_os_error()),
        }
    }

    /// The foreground process group, which is what agent detection reads the
    /// process table for.
    fn leader(&self) -> Option<libc::pid_t> {
        let pgrp = unsafe { libc::tcgetpgrp(self.0) };
        (pgrp > 0).then_some(pgrp)
    }
}

impl Drop for Adopted {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

/// The pty behind a pane: one this process opened, or one it inherited.
pub enum Master {
    Owned(Box<dyn MasterPty + Send>),
    Adopted(Adopted),
}

impl Master {
    pub fn try_clone_reader(&self) -> std::io::Result<Box<dyn Read + Send>> {
        match self {
            Master::Owned(m) => m
                .try_clone_reader()
                .map_err(|e| std::io::Error::other(e.to_string())),
            Master::Adopted(a) => Ok(Box::new(a.duplicate()?)),
        }
    }

    pub fn take_writer(&self) -> std::io::Result<Box<dyn Write + Send>> {
        match self {
            Master::Owned(m) => m
                .take_writer()
                .map_err(|e| std::io::Error::other(e.to_string())),
            Master::Adopted(a) => Ok(Box::new(a.duplicate()?)),
        }
    }

    pub fn resize(&self, size: PtySize) -> std::io::Result<()> {
        match self {
            Master::Owned(m) => m
                .resize(size)
                .map_err(|e| std::io::Error::other(e.to_string())),
            Master::Adopted(a) => a.resize(size),
        }
    }

    pub fn process_group_leader(&self) -> Option<libc::pid_t> {
        match self {
            Master::Owned(m) => m.process_group_leader(),
            Master::Adopted(a) => a.leader(),
        }
    }

    /// The descriptor, for handing on to the next binary.
    ///
    /// `None` where the pty library will not say, which is the one case a
    /// handoff cannot carry: better to refuse the whole thing than to exec and
    /// leave one pane with nothing behind it.
    pub fn raw_fd(&self) -> Option<RawFd> {
        match self {
            Master::Owned(m) => m.as_raw_fd(),
            Master::Adopted(a) => Some(a.0),
        }
    }

    /// Let this descriptor survive an `exec`.
    pub fn keep_across_exec(&self) -> std::io::Result<()> {
        let Some(fd) = self.raw_fd() else {
            return Err(std::io::Error::other("this pty has no descriptor to keep"));
        };
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        match unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } {
            0 => Ok(()),
            _ => Err(std::io::Error::last_os_error()),
        }
    }
}

/// A process this image inherited from the one it replaced.
///
/// `exec` keeps our children, so these are still ours to wait on -- the handle
/// that knew how went with the old image, and a pid is enough to rebuild it.
#[derive(Debug)]
pub struct Inherited {
    pid: libc::pid_t,
    ended: Option<ExitStatus>,
}

impl Inherited {
    pub fn new(pid: libc::pid_t) -> Self {
        Inherited { pid, ended: None }
    }

    fn reap(&mut self, block: bool) -> Option<ExitStatus> {
        if let Some(done) = &self.ended {
            return Some(done.clone());
        }
        let mut code = 0i32;
        let flags = if block { 0 } else { libc::WNOHANG };
        let got = unsafe { libc::waitpid(self.pid, &mut code, flags) };
        if got != self.pid {
            return None;
        }
        // The shape `WEXITSTATUS` and `WTERMSIG` read, written out because the
        // macros are not exposed as functions.
        let status = match code & 0x7f {
            0 => ExitStatus::with_exit_code(((code >> 8) & 0xff) as u32),
            // Killed by a signal. Reported the way a shell reports it, so a
            // pane that was terminated does not read as one that exited 0.
            signal => ExitStatus::with_exit_code(128 + signal as u32),
        };
        self.ended = Some(status.clone());
        Some(status)
    }
}

/// The child behind a pane: one this process spawned, or one it inherited.
pub enum Kid {
    Owned(Box<dyn Child + Send + Sync>),
    Inherited(Inherited),
}

impl Kid {
    pub fn kill(&mut self) -> std::io::Result<()> {
        match self {
            Kid::Owned(c) => c.kill(),
            Kid::Inherited(i) => {
                // The group, not the process: a shell's children are what is
                // actually holding the pty open, and killing only the shell
                // leaves them running with nowhere to write.
                unsafe { libc::kill(-i.pid, libc::SIGHUP) };
                match unsafe { libc::kill(i.pid, libc::SIGHUP) } {
                    0 => Ok(()),
                    _ => Err(std::io::Error::last_os_error()),
                }
            }
        }
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        match self {
            Kid::Owned(c) => c.wait(),
            Kid::Inherited(i) => i
                .reap(true)
                .ok_or_else(|| std::io::Error::other("no such child")),
        }
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        match self {
            Kid::Owned(c) => c.try_wait(),
            Kid::Inherited(i) => Ok(i.reap(false)),
        }
    }

    /// The pid, for writing down before an exec.
    pub fn pid(&self) -> Option<libc::pid_t> {
        match self {
            Kid::Owned(c) => c.process_id().map(|p| p as libc::pid_t),
            Kid::Inherited(i) => Some(i.pid),
        }
    }
}

/// Whether a descriptor is still open, so a handoff file naming a stale one is
/// refused rather than adopted into a pane with nothing behind it.
pub fn alive(fd: RawFd) -> bool {
    unsafe { libc::fcntl(fd, libc::F_GETFD) >= 0 }
}
