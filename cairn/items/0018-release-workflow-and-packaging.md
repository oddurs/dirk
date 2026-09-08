---
id: 18
title: Release workflow and packaging
type: chore
status: review
milestone: v1.0
assignee: oddurs
labels:
- deferred-homebrew
created: 2026-09-06
updated: 2026-09-07
priority: p1
area: packaging
---

## Problem
CI proves the tree builds; nothing produces something a person can install.
cairn has a release workflow and a Homebrew formula, and dirk has `cargo
install --path` and good wishes.

## Acceptance criteria
- [ ] A tagged release builds binaries for linux and macos
- [ ] NEWS is the changelog the release notes are cut from
- [ ] A source tarball a stranger could build from
- [ ] Checksums, so a download can be verified
- [ ] A Homebrew formula — deferred; see below

## 2026-09-07

## What the shape turned out to be

A tag is the trigger and NEWS is the source. `.github/workflows/release.yml`
checks that the tag, `Cargo.toml` and NEWS agree before it builds anything —
a release whose version disagrees with its changelog is worse than no release —
then builds five targets, tars each with the README, COPYING, NEWS and the
manual page, adds a source tarball from `make dist`, writes SHA256SUMS, and
cuts the release with the notes taken from the NEWS section for that version.

`git work release <VER>` prepares the version bump as an ordinary pull request;
`git work tag <VER>` tags main once it has landed. Nothing about a release
happens outside the normal loop except the tag.

## Two things that will not happen here

**crates.io.** The name `dirk` was taken in 2019 by an unrelated AWS SSM tool
and is not coming back. Publishing would mean shipping under a different crate
name than the binary, which is worse than not publishing; `cargo install
--git` works today and is honest about what it installs.

**Homebrew.** `oddurs/homebrew-tap` exists and has casks in it, but pointing
the release workflow at another repository needs a token with write access to
it, and that is a decision to make deliberately rather than as a side effect of
this. Deferred, on purpose.

## 2026-09-07

The workflow is written and lints clean; what it has not done yet is run,
because the v0.4.0 tag predates it. That is the one criterion still open, and
v0.5.0 settles it. The v0.4.0 release was published by hand from the same NEWS
section `scripts/news` cuts, and says so.
