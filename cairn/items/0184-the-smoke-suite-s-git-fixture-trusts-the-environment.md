---
id: 184
title: The smoke suite's git fixture trusts the environment
type: bug
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p2
area: mux
effort: s
---

## What happens

`a_repo_on()` in `tests/smoke.rs` builds a fixture repository with plain `git
init` / `git add` / `git commit` in a temporary directory, and does not clear
the git environment first.

`GIT_DIR` beats both `-C` and the working directory. With one set, `git init`
in the fixture does not create a repository there at all — it re-initialises
whatever `GIT_DIR` points at, warns that it is ignoring `--initial-branch`, and
returns success. The fixture ends up with no `.git`, and the `git add` and `git
commit` that follow operate on the ambient repository's index.

Demonstrated:

    $ GIT_DIR=../victim/.git git init -q -b zarquon
    warning: re-init: ignored --initial-branch=zarquon
    $ ls -d .git
    ls: .git: No such file or directory

`tests/session.rs` already knows this. Its `git_at()` clears `GIT_DIR`,
`GIT_WORK_TREE`, `GIT_COMMON_DIR` and `GIT_INDEX_FILE`, and says why: a suite
run from a git alias inherits one. `scripts/work` strips the same four before
`make check` for the same reason.

So the usual path is safe, and this is a hole rather than a live fire: it opens
when the tests are run from a git alias other than `work`, from a hook, or from
`git rebase --exec 'make check'`.

## What should happen

The fixture is a fixture wherever it is run from.

## Reproduction

1. `GIT_DIR=$PWD/.git cargo test --test smoke a_space_can_lead`

## Notes

The two suites now have two repository fixtures — `a_repo`/`git_at` in
`session.rs`, `a_repo_on` in `smoke.rs` — with the same job and different care
taken. The fix is to have one, in a place both can see, and for it to be the
careful one.

`smoke.rs` also still starts dirk in `CARGO_MANIFEST_DIR` by default, which is
the ambient-state dependence that produced two CI-only failures in v0.8. Worth
considering whether the default should be a fixture and the checkout the
exception.

## Acceptance criteria

- [ ] One repository fixture, shared by both suites
- [ ] It clears the git environment, and says why
- [ ] The suites pass with `GIT_DIR` set to something else
