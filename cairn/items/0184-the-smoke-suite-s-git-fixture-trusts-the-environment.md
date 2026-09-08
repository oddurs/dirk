---
id: 184
title: The smoke suite's git fixture trusts the environment
type: bug
status: done
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

- [x] One repository fixture, shared by both suites
- [x] It clears the git environment, and says why
- [x] The suites pass with `GIT_DIR` set to something else

## 2026-09-08

`tests/fixture/` — a directory rather than a file, so cargo takes it as a module
to include and not a suite to run. Both suites use it now, and the careful one
is the one that survived.

Three things guard it rather than one comment.

The command is built and then run, which lets a test assert the clearing
instead of trusting it. Asserted on the command because `GIT_DIR` is
process-global: a test that set it would be setting it for every other test in
the binary at the same time, and the race would be the flake.

`repo()` checks that the directory it just initialised has a `.git` in it. That
is the assertion that would have caught this directly, and it is one line: the
failure mode here is not an error, it is a warning nobody reads and an exit
status of zero.

And the whole suite was run with `GIT_DIR` pointing somewhere else. It passes.

The other half of the note — that `smoke.rs` still starts dirk in the checkout
by default — is left alone. It is a different property with different tests
depending on it, and folding it into this change would have made a fixture fix
into a change to what every test in the file is looking at.
