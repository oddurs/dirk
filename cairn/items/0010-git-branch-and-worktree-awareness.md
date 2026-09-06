---
id: 10
title: Git branch and worktree awareness
type: feature
status: backlog
milestone: v0.1
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: naming
---

## Problem
`name.rs` takes a branch argument and `main.rs` passes an empty string, so
`looks_auto_generated` cannot recognise a label that is just the branch name and
will treat it as hand-written. Worktrees are not distinguished from checkouts
either, and smali's tree coloured them differently for good reason.

## Acceptance criteria
- [ ] Branch resolved per project and cached, refreshed off the tick
- [ ] Worktrees marked distinctly in the sidebar
- [ ] A label equal to the branch name is adoptable
