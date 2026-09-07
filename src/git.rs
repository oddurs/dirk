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

/// One checkout of a repository, as `git worktree list` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub path: PathBuf,
    /// Empty on a detached head, which is a state rather than a name.
    pub branch: String,
    /// The one the repository was cloned into, as against one added beside it.
    pub main: bool,
}

/// Every checkout of the repository this directory belongs to.
///
/// The porcelain form because the human one is a table aligned for reading, and
/// a path with a space in it makes the columns a guess.
pub fn worktrees(dir: &Path) -> Vec<Tree> {
    let Some(out) = git(dir, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let mut trees = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch = String::new();
    for line in out.lines().chain(std::iter::once("")) {
        match line.split_once(' ') {
            Some(("worktree", p)) => {
                path = Some(PathBuf::from(p));
                branch.clear();
            }
            // `refs/heads/x` is where a branch lives; the name is the rest.
            Some(("branch", b)) => branch = b.trim_start_matches("refs/heads/").to_string(),
            _ if line.is_empty() => {
                if let Some(p) = path.take() {
                    trees.push(Tree {
                        // The first one listed is the repository proper. git
                        // says so by ordering rather than by marking it.
                        main: trees.is_empty(),
                        path: p,
                        branch: std::mem::take(&mut branch),
                    });
                }
            }
            _ => {}
        }
    }
    trees
}

/// Where a new worktree should go, given the repository it belongs to.
///
/// A sibling directory named for the repository and the branch, which is what
/// people do by hand: inside the repository it would be a directory git has to
/// be told to ignore, and somewhere central it would be a directory nobody
/// finds again.
pub fn beside(main: &Path, branch: &str) -> PathBuf {
    let name = main
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    // `feat/x` is a legal branch and an illegal directory component.
    let leaf = format!("{name}-{}", branch.replace('/', "-"));
    match main.parent() {
        Some(p) => p.join(leaf),
        None => PathBuf::from(leaf),
    }
}

/// Add a worktree, making the branch if it is not there.
///
/// Answers where it landed, or what git said about why it did not. The message
/// is git's own: it knows why better than dirk does, and repeating it in other
/// words would only make it harder to search for.
pub fn add(dir: &Path, branch: &str, at: &Path) -> Result<PathBuf, String> {
    // `-B` rather than `-b`: pointing an existing branch at a new worktree is
    // what you want when you come back to work you left, and `-b` refuses.
    let exists = git(
        dir,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    )
    .is_some();
    let mut args = vec!["worktree", "add"];
    if !exists {
        args.push("-b");
        args.push(branch);
    }
    let at_str = at.to_string_lossy().into_owned();
    args.push(&at_str);
    if exists {
        args.push(branch);
    }
    run(dir, &args).map(|_| at.to_path_buf())
}

/// Remove one, refusing while it holds work nobody has committed.
///
/// git refuses that by default and this passes the refusal along rather than
/// deciding for you: a worktree is a directory somebody has been working in,
/// and the case for removing it anyway is one only they can make.
pub fn remove(dir: &Path, at: &Path, force: bool) -> Result<(), String> {
    let at = at.to_string_lossy().into_owned();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&at);
    run(dir, &args).map(|_| ())
}

/// Run git and hand back what it said when it fails.
fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(match said.is_empty() {
        true => "git refused, and said nothing about why".into(),
        // One line: the first is what happened and the rest is advice for a
        // terminal, which this is not.
        false => said.lines().next().unwrap_or_default().to_string(),
    })
}

/// A read that has been asked for and not yet answered.
#[derive(Debug)]
pub struct Answer {
    pub dir: PathBuf,
    pub repo: Option<Repo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worktree_goes_beside_the_repository_it_belongs_to() {
        // What people do by hand. Inside the repository it is a directory git
        // has to be told to ignore; somewhere central it is one nobody finds.
        let main = Path::new("/Users/x/Code/dirk");
        assert_eq!(
            beside(main, "packaging"),
            Path::new("/Users/x/Code/dirk-packaging")
        );
    }

    #[test]
    fn a_branch_with_a_slash_in_it_is_still_one_directory() {
        // `feat/x` is a legal branch and an illegal path component, and the
        // failure is a directory called `x` inside one called `dirk-feat`.
        assert_eq!(
            beside(Path::new("/Code/dirk"), "feat/packaging"),
            Path::new("/Code/dirk-feat-packaging")
        );
    }
}
