---
id: 177
title: CI never builds what the release ships
type: chore
status: backlog
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

- [ ] Every target in `release.yml` is checked by `ci.yml`
- [ ] The list is in one place, or the two are checked against each other
- [ ] A target that fails to compile fails the pull request, not the tag
