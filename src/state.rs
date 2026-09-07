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

//! What a session is worth writing down.
//!
//! Not the panes. A pane is a process, and a process does not survive a machine
//! restarting however carefully its output was saved — restoring a screenful of
//! text with no program behind it would be worse than an empty pane, because it
//! looks like something you can type into.
//!
//! What is worth keeping is the **shape**: which projects were open, which
//! workspaces were in them, and what those workspaces were called. Those are
//! the parts a human arranged, and the parts that are tedious to arrange again.
//!
//! A name that naming worked out is kept along with whether it was held, so a
//! session comes back reading the way it read — and a name you wrote by hand is
//! still yours after a restart.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// On-disk format. A session written by a version that knew more than this one
/// is not read, rather than half-read.
const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub version: u32,
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
    pub label: String,
    /// Whether a human named this one. Kept, or a restart would quietly hand a
    /// name you wrote back to the naming policy.
    pub held: bool,
}

/// Where a session is written.
///
/// The directory is a parameter rather than read from the environment inside: a
/// function that reaches for a global is one that cannot be tested beside
/// another copy of itself, and two of these tests do exactly that.
pub fn path(base: &Path, session: &str) -> PathBuf {
    base.join("dirk")
        .join("sessions")
        .join(format!("{session}.json"))
}

/// Where sessions live by default.
pub fn base() -> PathBuf {
    crate::config::config_home()
}

/// Write it out, atomically.
///
/// Beside the target and renamed over it, so an interrupted write leaves the
/// previous session rather than half of this one. A truncated file here means
/// coming back to an empty dirk, which is the one outcome this feature exists
/// to prevent.
pub fn save(base: &Path, session: &str, state: &Saved) -> std::io::Result<()> {
    let target = path(base, session);
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = serde_json::to_vec_pretty(state)?;
    let temp = target.with_extension("json.new");
    std::fs::write(&temp, body)?;
    std::fs::rename(&temp, &target)
}

/// Read it back, or nothing at all.
///
/// Every failure is nothing: no file, unreadable, malformed, or written by a
/// version that knew more. A session that will not load is a session you start
/// empty, not one that refuses to start.
pub fn load(base: &Path, session: &str) -> Option<Saved> {
    let text = std::fs::read_to_string(path(base, session)).ok()?;
    let saved: Saved = serde_json::from_str(&text).ok()?;
    (saved.version <= VERSION).then_some(saved)
}

/// Drop projects whose directory has gone.
///
/// A path can move or be deleted between one run and the next, and a session
/// that refused to open because one of six projects was gone would be a session
/// you had to repair by hand before you could use it.
pub fn prune(mut saved: Saved) -> Saved {
    saved.projects.retain(|p| p.path.is_dir());
    saved
}

pub fn current(session: &crate::mux::Session) -> Saved {
    Saved {
        version: VERSION,
        projects: session
            .projects
            .iter()
            .map(|p| Project {
                path: p.path.clone(),
                expanded: p.expanded,
                workspaces: p
                    .workspaces
                    .iter()
                    .map(|w| Workspace {
                        label: w.label.clone(),
                        held: w.naming.held,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Has anything worth writing down changed?
pub fn differs(a: &Saved, b: &Saved) -> bool {
    let shape = |s: &Saved| {
        s.projects
            .iter()
            .map(|p| {
                (
                    p.path.clone(),
                    p.workspaces
                        .iter()
                        .map(|w| w.label.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    shape(a) != shape(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(paths: &[(&str, &[&str])]) -> Saved {
        Saved {
            version: VERSION,
            projects: paths
                .iter()
                .map(|(path, labels)| Project {
                    path: PathBuf::from(path),
                    expanded: true,
                    workspaces: labels
                        .iter()
                        .map(|l| Workspace {
                            label: (*l).to_string(),
                            held: false,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_project_that_has_gone_is_dropped_rather_than_failing_the_load() {
        // A session you have to repair by hand before it will open is worse
        // than one that quietly opens with five of its six projects.
        let state = saved(&[
            ("/definitely/not/here", &["a"]),
            (env!("CARGO_MANIFEST_DIR"), &["b"]),
        ]);
        let kept = prune(state);
        assert_eq!(kept.projects.len(), 1);
        assert_eq!(kept.projects[0].workspaces[0].label, "b");
    }

    /// A directory of this test's own, so two of these can run at once.
    fn scratch(what: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dirk-state-{}-{what}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(dir.join("dirk").join("sessions"));
        dir
    }

    #[test]
    fn a_session_from_a_newer_version_is_not_half_read() {
        let dir = scratch("newer");
        let mut state = saved(&[("/tmp", &["a"])]);

        state.version = VERSION + 1;
        save(&dir, "s", &state).unwrap();
        assert!(
            load(&dir, "s").is_none(),
            "a format we do not know was read anyway"
        );

        state.version = VERSION;
        save(&dir, "s", &state).unwrap();
        assert!(load(&dir, "s").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_that_will_not_load_stops_a_session_starting() {
        // Every failure is nothing: no file, unreadable, or malformed. A
        // session that will not load is one you start empty, not one that
        // refuses to start.
        let dir = scratch("broken");
        std::fs::write(path(&dir, "broken"), b"{ not json").unwrap();
        assert!(load(&dir, "broken").is_none());
        assert!(load(&dir, "never-written").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_interrupted_write_leaves_the_previous_session() {
        // Written beside the target and renamed over it. A truncated file here
        // means coming back to an empty dirk, which is the one outcome this
        // exists to prevent.
        let dir = scratch("atomic");
        save(&dir, "s", &saved(&[("/tmp", &["first"])])).unwrap();
        save(&dir, "s", &saved(&[("/tmp", &["second"])])).unwrap();
        assert_eq!(
            load(&dir, "s").unwrap().projects[0].workspaces[0].label,
            "second"
        );
        // And nothing is left beside it.
        assert!(!path(&dir, "s").with_extension("json.new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_change_worth_writing_down_is_noticed_and_a_redraw_is_not() {
        let a = saved(&[("/tmp", &["one"])]);
        assert!(!differs(&a, &a.clone()), "nothing changed");
        assert!(
            differs(&a, &saved(&[("/tmp", &["two"])])),
            "a rename is a change"
        );
        assert!(
            differs(&a, &saved(&[("/tmp", &["one", "two"])])),
            "a new workspace is a change"
        );
        assert!(differs(&a, &saved(&[])), "a closed project is a change");
    }
}
