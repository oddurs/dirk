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
use std::io::Write;
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
    /// Which checkout of the project it was in.
    ///
    /// A project is a repository, so restoring one directory is not enough:
    /// the worktrees somebody had open are part of the arrangement, and
    /// putting every space back in the repository proper would quietly undo
    /// the reason they made them.
    #[serde(default)]
    pub at: PathBuf,
    /// Whether a human named this one. Kept, or a restart would quietly hand a
    /// name you wrote back to the naming policy.
    pub held: bool,
    /// The harness that was working here, and its own name for the
    /// conversation.
    ///
    /// The panes themselves are not restored — a process is a process — but a
    /// conversation is not a process, and the harness can pick one up again.
    /// Restoring the shape of the work while losing the work is close to the
    /// worst available place to stop.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub agent_session: Option<String>,
    /// The names of its tabs, in order.
    ///
    /// Only the names. What was running in one is a process, and a screenful of
    /// text with nothing behind it is worse than an empty tab -- the same
    /// argument the workspaces themselves are restored under.
    #[serde(default)]
    pub tabs: Vec<String>,
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
    let Some(dir) = target.parent() else {
        return Err(std::io::Error::other("nowhere to write"));
    };
    std::fs::create_dir_all(dir)?;

    let body = serde_json::to_vec_pretty(state)?;
    let temp = target.with_extension("json.new");
    // On disk before the rename, not just in the page cache. A rename is atomic
    // for anything reading it, but a machine that loses power between the two
    // can come back to a name pointing at a file with nothing in it -- which is
    // the empty dirk this is here to prevent, arrived at the long way round.
    let written = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&body)?;
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&temp, &target) {
        // Otherwise a directory that cannot be renamed into slowly fills with
        // the sessions that could not be written.
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    // The rename itself, so the entry survives the same power loss the contents
    // now do. Best effort: not every filesystem lets you open a directory.
    if let Ok(dir) = std::fs::File::open(dir) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// What was on disk.
pub enum Stored {
    /// Nothing has been written for this session yet.
    Fresh,
    /// Something is there and could not be used. Kept, deliberately: the file
    /// may be the only record of an arrangement, and a newer dirk that wrote it
    /// will want it back when you next run that one.
    Unreadable,
    Saved(Saved),
}

/// Read it back.
///
/// A session that will not load is a session you start empty, not one that
/// refuses to start -- but "there is nothing here" and "there is something here
/// I cannot read" are different answers, and treating them alike is how a file
/// written by a newer dirk gets replaced by an older one a second after it
/// starts.
pub fn read(base: &Path, session: &str) -> Stored {
    let text = match std::fs::read_to_string(path(base, session)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Stored::Fresh,
        Err(_) => return Stored::Unreadable,
    };
    match serde_json::from_str::<Saved>(&text) {
        Ok(saved) if saved.version <= VERSION => Stored::Saved(saved),
        _ => Stored::Unreadable,
    }
}

/// Drop projects whose directory has gone.
///
/// A path can move or be deleted between one run and the next, and a session
/// that refused to open because one of six projects was gone would be a session
/// you had to repair by hand before you could use it.
///
/// Gone means the filesystem said so. Anything else -- a mount that has not
/// come up, a server that is not answering, a directory you cannot look in
/// right now -- keeps the project, because those all come back and a project
/// dropped here is a project erased by the next write.
pub fn prune(mut saved: Saved) -> Saved {
    /// Gone means the filesystem said so. Anything else -- a mount that has
    /// not come up, a server that is not answering, a directory you cannot
    /// look in right now -- keeps it, because those all come back.
    fn there(path: &Path) -> bool {
        match std::fs::metadata(path) {
            Ok(m) => m.is_dir(),
            Err(e) => e.kind() != std::io::ErrorKind::NotFound,
        }
    }

    saved.projects.retain_mut(|p| {
        let repo_there = there(&p.path);
        let had = !p.workspaces.is_empty();
        // A checkout that has gone takes its spaces and nothing else: removing
        // a worktree is an ordinary thing to do and the repository it came from
        // is still there. A space saved before spaces knew their own checkout
        // is wherever the project is, which is what it always was.
        p.workspaces.retain(|w| match w.at.as_os_str().is_empty() {
            true => repo_there,
            false => there(&w.at),
        });
        match had {
            // Every checkout it had is gone, so there is nothing left to open.
            true => !p.workspaces.is_empty(),
            false => repo_there,
        }
    });
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
                        at: w.at.clone(),
                        held: w.naming.held,
                        agent: w.agent_session.as_ref().map(|(k, _)| k.clone()),
                        agent_session: w.agent_session.as_ref().map(|(_, s)| s.clone()),
                        tabs: (0..w.tabs.len()).map(|i| w.tab_label(i)).collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Has anything worth writing down changed?
///
/// Everything that is written, not only what is visible. Releasing a hold
/// leaves the label alone and changes only `held`, and a comparison that
/// ignored it would decline to write -- so the release would last exactly as
/// long as the session did.
pub fn differs(a: &Saved, b: &Saved) -> bool {
    let shape = |s: &Saved| {
        s.projects
            .iter()
            .map(|p| {
                (
                    p.path.clone(),
                    p.expanded,
                    p.workspaces
                        .iter()
                        .map(|w| {
                            (
                                w.label.clone(),
                                w.at.clone(),
                                w.held,
                                w.tabs.clone(),
                                // Or a conversation reported after the last
                                // write would never reach the file, and the
                                // restart it exists for would find nothing.
                                w.agent.clone(),
                                w.agent_session.clone(),
                            )
                        })
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
                            at: PathBuf::new(),
                            tabs: Vec::new(),
                            label: (*l).to_string(),
                            held: false,
                            agent: None,
                            agent_session: None,
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
            matches!(read(&dir, "s"), Stored::Unreadable),
            "a format we do not know was read anyway"
        );

        state.version = VERSION;
        save(&dir, "s", &state).unwrap();
        assert!(matches!(read(&dir, "s"), Stored::Saved(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_that_will_not_load_stops_a_session_starting() {
        // A session that will not load is one you start empty, not one that
        // refuses to start -- but the two reasons for starting empty are not
        // the same, and only one of them means the file is ours to replace.
        let dir = scratch("broken");
        std::fs::write(path(&dir, "broken"), b"{ not json").unwrap();
        assert!(
            matches!(read(&dir, "broken"), Stored::Unreadable),
            "a file we cannot read is not a file that is not there"
        );
        assert!(matches!(read(&dir, "never-written"), Stored::Fresh));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_checkout_that_has_gone_does_not_take_the_repository_with_it() {
        // Removing a worktree is an ordinary thing to do. The repository it was
        // a worktree of is still there, and so is the work in it.
        let dir = scratch("checkout");
        let here = dir.join("here");
        std::fs::create_dir_all(&here).unwrap();
        let mut state = Saved {
            version: VERSION,
            projects: vec![Project {
                path: here.clone(),
                expanded: true,
                workspaces: vec![
                    Workspace {
                        label: "main".into(),
                        at: here.clone(),
                        held: false,
                        agent: None,
                        agent_session: None,
                        tabs: Vec::new(),
                    },
                    Workspace {
                        label: "gone".into(),
                        at: dir.join("removed"),
                        held: false,
                        agent: None,
                        agent_session: None,
                        tabs: Vec::new(),
                    },
                ],
            }],
        };
        state = prune(state);
        assert_eq!(state.projects.len(), 1, "the repository went too");
        assert_eq!(state.projects[0].workspaces.len(), 1);
        assert_eq!(state.projects[0].workspaces[0].label, "main");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_whose_every_checkout_has_gone_is_dropped() {
        // Nothing left to open. Keeping it would be a row in the nav that
        // cannot be entered.
        let dir = scratch("allgone");
        let state = prune(Saved {
            version: VERSION,
            projects: vec![Project {
                path: dir.join("repo"),
                expanded: true,
                workspaces: vec![Workspace {
                    label: "x".into(),
                    at: dir.join("repo-side"),
                    held: false,
                    agent: None,
                    agent_session: None,
                    tabs: Vec::new(),
                }],
            }],
        });
        assert!(state.projects.is_empty(), "an unopenable project was kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_that_is_only_unreachable_is_kept() {
        // Deleted and "not mounted this minute" look alike from here, and a
        // project dropped for the second reason is erased by the next write.
        let dir = scratch("prune");
        let there = dir.join("here");
        std::fs::create_dir_all(&there).unwrap();
        let kept = prune(Saved {
            version: VERSION,
            projects: vec![
                Project {
                    path: there.clone(),
                    expanded: true,
                    workspaces: Vec::new(),
                },
                Project {
                    path: dir.join("gone"),
                    expanded: true,
                    workspaces: Vec::new(),
                },
            ],
        });
        assert_eq!(kept.projects.len(), 1, "the one that is there was dropped");
        assert_eq!(kept.projects[0].path, there);
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
        let Stored::Saved(back) = read(&dir, "s") else {
            panic!("the second write did not land")
        };
        assert_eq!(back.projects[0].workspaces[0].label, "second");
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

        // Releasing a hold leaves the label alone and changes only `held`. A
        // comparison that missed it would decline to write, and the release
        // would last exactly as long as the session did.
        let mut released = a.clone();
        released.projects[0].workspaces[0].held = !a.projects[0].workspaces[0].held;
        assert!(differs(&a, &released), "a released hold is a change");

        let mut collapsed = a.clone();
        collapsed.projects[0].expanded = !a.projects[0].expanded;
        assert!(differs(&a, &collapsed), "a collapsed project is a change");
        assert!(
            differs(&a, &saved(&[("/tmp", &["one", "two"])])),
            "a new workspace is a change"
        );
        assert!(differs(&a, &saved(&[])), "a closed project is a change");
    }
}
