---
id: 95
title: Rows say what they hold without being opened
type: feature
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: nav
effort: s
---

## Problem
A row with children says so only by having them, and only once expanded.  A
folded one is silent about whether there is anything under it at all, so the
disclosure is a thing you try rather than a thing you read.

## Shape
The mark goes at the right edge rather than the left: the left is where the
state glyph and the number are, which is where the eye starts, and a disclosure
there competes with the two things that matter more.

## Acceptance criteria
- [x] A row with children carries a disclosure mark at its right edge
- [x] A row with none carries nothing, rather than a dimmed mark
- [x] Clicking the mark folds; clicking the row goes there
