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

//! What git says about a directory.
//!
//! Two facts, and dirk wants them for the same reason: **which branch** is what
//! a checkout is for, and **whether it is a worktree** is why there are three
//! checkouts of one repository open at once. Running several agents in parallel
//! means several worktrees, and a nav that cannot tell them apart is a nav
//! showing the same project name three times.
//!
//! Reading them shells out, so it never happens on the drawing thread. A `git`
//! that has gone to a network remote or is waiting on an index lock would stall
//! the whole program, and the answer is wanted for a caption.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repo {
    /// Empty on a detached head, which is a state and not a name.
    pub branch: String,
    pub worktree: bool,
}

/// How long an answer is good for. A branch changes a few times an hour at
/// most, and asking more often costs a process each time.
pub const REFRESH: Duration = Duration::from_secs(15);

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        // A pager or an editor would inherit this process's terminal, which is
        // the alternate screen dirk is drawing on.
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Read the branch and worktree status of a directory, or `None` when it is not
/// a repository at all.
pub fn read(dir: &Path) -> Option<Repo> {
    // A worktree's git dir lives inside the main repository's, under
    // `worktrees/`. That is the check git itself documents, and it is one
    // process rather than parsing `git worktree list`.
    let git_dir = git(dir, &["rev-parse", "--absolute-git-dir"])?;
    let worktree = Path::new(&git_dir)
        .components()
        .any(|c| c.as_os_str() == "worktrees");

    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|b| b != "HEAD")
        .unwrap_or_default();

    Some(Repo { branch, worktree })
}

/// A read that has been asked for and not yet answered.
#[derive(Debug)]
pub struct Answer {
    pub dir: PathBuf,
    pub repo: Option<Repo>,
}
