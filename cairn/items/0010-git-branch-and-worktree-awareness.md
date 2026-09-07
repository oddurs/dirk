---
id: 10
title: Git branch and worktree awareness
type: feature
status: done
milestone: v0.2
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: git
---

## Problem
`name.rs` takes a branch argument and `main.rs` passes an empty string, so
`looks_auto_generated` cannot recognise a label that is just the branch name and
will treat it as hand-written. Worktrees are not distinguished from checkouts
either, and smali's tree coloured them differently for good reason.

## Acceptance criteria
- [x] Branch resolved per project and cached, refreshed off the tick
- [x] Worktrees marked distinctly in the sidebar
- [x] A label equal to the branch name is adoptable
