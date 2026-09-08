---
id: 177
title: CI never builds what the release ships
type: chore
status: done
milestone: v1.0
created: 2026-09-08
updated: 2026-09-08
priority: p1
area: packaging
effort: s
---

## Problem

`release.yml` builds five targets. CI builds one, plus a macOS test job. So
four of the five are compiled for the first time by the tag that publishes
them, and a target that stops compiling is found at the worst possible moment —
partway through a release, with the tag already pushed.

This is not hypothetical: `dirk` does not currently compile for
`aarch64-unknown-linux-gnu`, and has not since the window-title work landed.
Nothing noticed.

## Proposal

A `cross` job in `ci.yml` running `cargo check --target T` for each of the five
release targets. `check` rather than `build`: this is about the code being
valid for the target, and a check needs the standard library for it but no
linker and no cross toolchain.

It is not free — five more toolchain downloads — so it is one job with a
matrix, cached, and it does not run the tests.

## Acceptance criteria

- [x] Every target in `release.yml` is checked by `ci.yml`
- [x] The list is in one place, or the two are checked against each other
- [x] A target that fails to compile fails the pull request, not the tag

## 2026-09-08

One list, not two. The targets are read out of `release.yml` by the job that
uses them, because a second copy in `ci.yml` is exactly the thing that goes
stale — and going stale is the whole failure this job exists to prevent.

The scrape asserts what it found. A `release.yml` somebody reformatted would
otherwise leave the matrix empty, every job passing, and nobody checking
anything; fewer than two targets is an error rather than a quiet afternoon.

`cargo check` and not `--all-targets`: the tarball holds the binary, so the
question is whether the program is valid for the target. Cross-compiling the
test binaries would put the dev-dependencies in the way of that question.
