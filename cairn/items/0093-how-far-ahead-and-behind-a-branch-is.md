---
id: 93
title: How far ahead and behind a branch is
type: feature
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: git
effort: m
---

## Problem
The nav says which branch a space is on and nothing about where that branch
stands.  Twenty-two commits behind is the difference between work you can ship
and work that is about to conflict, and finding out means leaving.

## Shape
`git rev-list --count --left-right @{upstream}...HEAD` gives both numbers in one
process, which is already how the branch itself is read -- the same call site,
the same fifteen-second cache, no new cost per pane.

Drawn as `↑6 ↓1`, and drawn as nothing when both are zero: a pair of zeroes is
not information, which is the rule the rail already follows for the attention
counts.

A branch with no upstream has no answer rather than a zero.  Those are
different, and saying "0" for "there is nothing to compare against" is the more
misleading of the two.

## Acceptance criteria
- [ ] Ahead and behind counts beside the branch
- [ ] Nothing drawn when both are zero, or when there is no upstream
- [ ] One process per project per refresh, not one per space
- [ ] Never on the drawing thread
