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

//! Repositories for tests to read back, made the careful way.
//!
//! Shared by both suites because there were two of these and they took
//! different amounts of care: the one that knew about `GIT_DIR` was in
//! `session.rs`, and the one that did not was the one written later.
//!
//! Under `tests/fixture/` rather than `tests/fixture.rs` so cargo treats it as
//! a module to include rather than a suite to run.

#![allow(dead_code)] // Each suite uses the part of this it needs.

use std::path::{Path, PathBuf};
use std::process::Output;

/// Run git against a fixture, and against nothing else.
///
/// The environment is cleared because `GIT_DIR` beats both `-C` and the working
/// directory — with one set, `git init` does not make a repository where it was
/// asked to, it re-initialises whatever `GIT_DIR` points at, warns that it is
/// ignoring `--initial-branch`, and exits zero. Everything after that runs
/// against somebody's real index.
///
/// A suite run from a git alias inherits one, and `git work` is a git alias.
/// It strips these before `make check` for this reason; so does this, because a
/// fixture that is only safe when it is launched correctly is not safe.
///
/// Identity comes from the environment rather than from whoever the machine
/// thinks is running: a commit is what most of these need to exist at all, and
/// CI has no `user.email` to make one with.
pub fn git(dir: &Path, args: &[&str]) -> Output {
    command(dir, args).output().expect("git")
}

/// What every one of the four is: a variable that would aim git somewhere else.
const INHERITED: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
];

/// Built rather than run, so the clearing can be asserted rather than trusted.
fn command(dir: &Path, args: &[&str]) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    for var in INHERITED {
        cmd.env_remove(var);
    }
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com");
    cmd
}

/// The same, and it must have worked.
///
/// For the steps that build the fixture itself. A failure there is not a test
/// result, it is a test that never ran, and it should say so where it happened
/// rather than as a puzzle three assertions later.
pub fn must(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository at `dir`, on `branch`, with one commit on it.
///
/// One commit, because a branch with nothing on it is unborn: `rev-parse HEAD`
/// has no answer, and dirk reads a branch by asking that.
pub fn repo(dir: &Path, branch: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("repo dir");
    must(dir, &["init", "-q", "-b", branch]);
    // Because the way this fails is not a failure. `git init` under a stray
    // `GIT_DIR` re-initialises whatever that points at, says so in a warning
    // nobody reads, and exits zero -- leaving a fixture with no repository in
    // it and every command after this one running against somebody else's.
    assert!(
        dir.join(".git").exists(),
        "no repository in {}: something aimed git elsewhere",
        dir.display()
    );
    std::fs::write(dir.join("a.txt"), b"one\n").expect("a file");
    must(dir, &["add", "-A"]);
    must(dir, &["commit", "-qm", "one"]);
    dir.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_git_environment_is_cleared_and_not_merely_overridden() {
        // Asserted on the command rather than by running one: `GIT_DIR` is
        // process-global, and a test that set it would be setting it for every
        // other test in the binary at the same time.
        //
        // `-C` is not enough and neither is a working directory. `GIT_DIR`
        // beats both, so the only thing that works is its absence.
        let cmd = command(Path::new("/nowhere"), &["status"]);
        let cleared: Vec<&str> = cmd
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .filter_map(|(name, _)| name.to_str())
            .collect();
        for var in INHERITED {
            assert!(cleared.contains(var), "{var} would be inherited");
        }
    }

    #[test]
    fn a_fixture_repository_is_a_repository() {
        let dir = std::env::temp_dir().join("dirk-fixture").join(format!(
            "{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        repo(&dir, "zarquon-fixture");
        assert!(dir.join(".git").exists());
        let head = git(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
        assert_eq!(
            String::from_utf8_lossy(&head.stdout).trim(),
            "zarquon-fixture"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
