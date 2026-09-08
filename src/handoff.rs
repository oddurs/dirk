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

//! Replacing the binary without ending the work.
//!
//! Upgrading meant stopping the session, which meant ending every shell and
//! every agent. So people did not upgrade while they were working, which is
//! always — and a long-running session is exactly the thing that makes an
//! upgrade expensive and exactly the thing dirk is for.
//!
//! ## How
//!
//! `exec`, not a second process. The server clears `FD_CLOEXEC` on every pty it
//! holds, writes down which descriptor and which pid belong to which pane, and
//! replaces its own image with the new binary. Same process, so every shell and
//! agent keeps the parent it had; same descriptors, so the ptys they are
//! talking through are the ones they were already talking through. Nothing is
//! passed between processes because there is only ever one.
//!
//! Handing the descriptors over a socket to a *new* process would allow a
//! rollback after the new server had started and failed. It would also mean
//! `SCM_RIGHTS`, two live servers racing for one listening socket, and reparenting
//! every child. The failure that actually happens — the new binary is the wrong
//! architecture, or truncated, or not there — is caught by running it once
//! before committing to it, and a failed `exec` simply returns.
//!
//! ## What does not survive
//!
//! Documented rather than discovered, because a caller waiting on an agent
//! through a handoff needs to know its wait ended rather than silently never
//! returning:
//!
//! - **Client connections.** Every attached terminal is dropped and reconnects.
//! - **Outstanding waits.** Ended with an error naming the handoff.
//! - **Observers and controllers.** Dropped; their streams end with a reason.
//! - **Scrollback.** The screen comes across; what scrolled off it does not.
//!
//! What does survive is the running processes, which is the whole point.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The format the two images agree on. A handoff written by a version that
/// knew more than this one is refused rather than half-read -- and refusing
/// means the old server carries on, which is the safe direction.
pub const VERSION: u32 = 1;

/// Where the note is left between the two images of one process.
pub fn path(session: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dirk-handoff-{session}.json"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub version: u32,
    pub session: String,
    /// The listening socket, kept open across the exec so no connection is
    /// refused in the gap.
    pub listener: i32,
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub path: PathBuf,
    pub expanded: bool,
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: u64,
    pub label: String,
    pub at: PathBuf,
    pub held: bool,
    pub agent: Option<(String, String)>,
    pub tab: usize,
    pub tabs: Vec<Tab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: u64,
    pub label: String,
    pub focus: u64,
    pub tree: Tree,
    pub panes: Vec<Pane>,
}

/// The split tree, as data.
///
/// `ratatui`'s `Constraint` is not serialisable and dirk only ever uses
/// `Fill(n)`, so a weight is the whole of what has to cross.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tree {
    Leaf(u64),
    Split {
        cols: bool,
        children: Vec<(u16, Tree)>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: u64,
    /// The descriptor this pane's pty is on, in the process both images share.
    pub fd: i32,
    pub pid: i32,
    pub rows: u16,
    pub cols: u16,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub label: Option<String>,
    pub agent_name: Option<String>,
    /// The screen, as the escape sequences that reproduce it.
    ///
    /// Not the scrollback: `contents_formatted` is what is on screen, which is
    /// what somebody is looking at. Carrying the scrollback would mean carrying
    /// megabytes through a file for the sake of what has already gone past.
    pub screen: String,
    pub modes: Modes,
}

/// The terminal modes a program turned on, which dirk has to know about to
/// keep behaving correctly toward it.
///
/// Forgetting these is not cosmetic. A pane whose program asked for mouse
/// reporting and is no longer believed to have asked stops receiving clicks; a
/// pane that asked for bracketed paste starts receiving pastes as typing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Modes {
    pub mouse: u8,
    pub mouse_encoding: u8,
    pub bracketed_paste: bool,
    pub application_cursor: bool,
    pub application_keypad: bool,
    pub hide_cursor: bool,
    pub title: Option<String>,
}

impl Modes {
    /// The escape sequences that put a fresh parser back into these modes.
    ///
    /// Replayed rather than reconstructed, because vt100 keeps this state
    /// privately and the only way in is the way a program would have done it.
    pub fn sequences(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let set = |out: &mut Vec<u8>, code: &str, on: bool| {
            out.extend_from_slice(b"\x1b[?");
            out.extend_from_slice(code.as_bytes());
            out.push(if on { b'h' } else { b'l' });
        };
        // In the order a program would: the mode, then its encoding, so a
        // terminal that treats the second as meaningless without the first
        // does not drop it.
        match self.mouse {
            0 => {}
            1 => set(&mut out, "9", true),
            2 => set(&mut out, "1000", true),
            3 => set(&mut out, "1002", true),
            _ => set(&mut out, "1003", true),
        }
        match self.mouse_encoding {
            1 => set(&mut out, "1005", true),
            2 => set(&mut out, "1006", true),
            _ => {}
        }
        if self.bracketed_paste {
            set(&mut out, "2004", true);
        }
        if self.application_cursor {
            set(&mut out, "1", true);
        }
        if self.hide_cursor {
            set(&mut out, "25", false);
        }
        if self.application_keypad {
            out.extend_from_slice(b"\x1b=");
        }
        if let Some(title) = &self.title {
            out.extend_from_slice(b"\x1b]2;");
            out.extend_from_slice(title.as_bytes());
            out.push(0x07);
        }
        out
    }
}

/// Write the note, readable only by the person running dirk.
///
/// It names descriptors and pids of a live session, and on a shared machine
/// `/tmp` is shared. The contents are useless to anybody who is not this
/// process, and there is no reason to publish them anyway.
pub fn write(session: &str, note: &Handoff) -> std::io::Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let target = path(session);
    let _ = std::fs::remove_file(&target);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&target)?;
    file.write_all(&serde_json::to_vec(note)?)?;
    file.sync_all()?;
    Ok(target)
}

/// Read it, and take it away.
///
/// Removed on the way past whatever happens next: a note left behind is one a
/// later start would try to adopt, with descriptors that belong to nothing.
pub fn read(at: &Path) -> Option<Handoff> {
    let body = std::fs::read(at).ok();
    let _ = std::fs::remove_file(at);
    let note: Handoff = serde_json::from_slice(&body?).ok()?;
    (note.version == VERSION).then_some(note)
}

/// Read the whole session out, ready to be written down.
///
/// Fails rather than half-succeeds: a pty whose descriptor the library will not
/// name is a pane that would arrive on the other side with nothing behind it,
/// and one such pane is worth refusing the handoff for. The caller carries on
/// running, which is the point of checking here.
pub fn capture(
    session: &crate::mux::Session,
    name: &str,
    listener: i32,
) -> Result<Handoff, String> {
    let mut projects = Vec::new();
    for p in &session.projects {
        let mut workspaces = Vec::new();
        for w in &p.workspaces {
            let mut tabs = Vec::new();
            for t in &w.tabs {
                let mut panes = Vec::new();
                for pane in &t.panes {
                    panes.push(capture_pane(pane)?);
                }
                tabs.push(Tab {
                    id: t.id,
                    label: t.label.clone(),
                    focus: t.focus,
                    tree: tree_of(&t.tree),
                    panes,
                });
            }
            workspaces.push(Workspace {
                id: w.id,
                label: w.label.clone(),
                at: w.at.clone(),
                held: w.naming.held,
                agent: w.agent_session.clone(),
                tab: w.tab,
                tabs,
            });
        }
        projects.push(Project {
            path: p.path.clone(),
            expanded: p.expanded,
            workspaces,
        });
    }
    Ok(Handoff {
        version: VERSION,
        session: name.to_string(),
        listener,
        projects,
    })
}

fn capture_pane(pane: &crate::mux::Pane) -> Result<Pane, String> {
    let fd = pane
        .master
        .raw_fd()
        .ok_or_else(|| format!("pane {} has a pty with no descriptor", pane.id))?;
    let pid = pane
        .pid()
        .ok_or_else(|| format!("pane {} has no process to keep", pane.id))?;
    let term = pane
        .term
        .lock()
        .map_err(|_| format!("pane {} is locked", pane.id))?;
    let screen = term.screen();
    let (rows, cols) = screen.size();
    let modes = Modes {
        mouse: match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::None => 0,
            vt100::MouseProtocolMode::Press => 1,
            vt100::MouseProtocolMode::PressRelease => 2,
            vt100::MouseProtocolMode::ButtonMotion => 3,
            vt100::MouseProtocolMode::AnyMotion => 4,
        },
        mouse_encoding: match screen.mouse_protocol_encoding() {
            vt100::MouseProtocolEncoding::Default => 0,
            vt100::MouseProtocolEncoding::Utf8 => 1,
            vt100::MouseProtocolEncoding::Sgr => 2,
        },
        bracketed_paste: screen.bracketed_paste(),
        application_cursor: screen.application_cursor(),
        application_keypad: screen.application_keypad(),
        hide_cursor: screen.hide_cursor(),
        title: term.callbacks().title.clone(),
    };
    Ok(Pane {
        id: pane.id,
        fd,
        pid,
        rows,
        cols,
        cwd: pane.cwd.clone(),
        argv: pane.argv.clone(),
        label: pane.label.clone(),
        agent_name: pane.agent_name.clone(),
        screen: crate::clipboard::base64(&screen.contents_formatted()),
        modes,
    })
}

fn tree_of(node: &crate::mux::tree::Node) -> Tree {
    use ratatui::layout::Constraint;
    match node {
        crate::mux::tree::Node::Leaf(id) => Tree::Leaf(*id),
        crate::mux::tree::Node::Split { dir, children } => Tree::Split {
            cols: matches!(dir, crate::mux::Dir::Cols),
            children: children
                .iter()
                .map(|(c, n)| {
                    let weight = match c {
                        Constraint::Fill(w) => *w,
                        // Nothing else is ever built, and a share of one is the
                        // answer that loses least if that changes.
                        _ => 1,
                    };
                    (weight, tree_of(n))
                })
                .collect(),
        },
    }
}

/// And back the other way, when the new image is rebuilding the tab.
pub fn tree_into(tree: &Tree) -> crate::mux::tree::Node {
    use ratatui::layout::Constraint;
    match tree {
        Tree::Leaf(id) => crate::mux::tree::Node::Leaf(*id),
        Tree::Split { cols, children } => crate::mux::tree::Node::Split {
            dir: match cols {
                true => crate::mux::Dir::Cols,
                false => crate::mux::Dir::Rows,
            },
            children: children
                .iter()
                .map(|(w, n)| (Constraint::Fill(*w), tree_into(n)))
                .collect(),
        },
    }
}

/// The screen, back out of the note.
pub fn unbase64(text: &str) -> Vec<u8> {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut have = 0u32;
    let mut out = Vec::new();
    for c in text.bytes().filter(|c| *c != b'=') {
        let Some(v) = A.iter().position(|a| *a == c) else {
            continue;
        };
        bits = (bits << 6) | v as u32;
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
        }
    }
    out
}
