---
id: 18
title: Release workflow and packaging
type: chore
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: packaging
---

## Problem
CI proves the tree builds; nothing produces something a person can install.
cairn has a release workflow and a Homebrew formula, and dirk has `cargo
install --path` and good wishes.

## Acceptance criteria
- [ ] A tagged release builds binaries for linux and macos
- [ ] A Homebrew formula, so installing dirk is as easy as installing herdr was
- [ ] NEWS is the changelog the release notes are cut from
