---
id: 26
title: Sidebar scrolling and keyboard selection
type: feature
status: done
milestone: v0.2
assignee: oddurs
depends_on:
- 50
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: nav
effort: m
---

## Problem
The nav is drawn top to bottom until it runs out of terminal, and then it stops.
Ten spaces on a short terminal means the agents section does not exist. There is
also no way to move through the nav from the keyboard without moving focus,
which is what a nav is for.

## Proposal
A selection that is distinct from focus: moving it does not change which pane
gets keys, and Enter commits it. Scrolling follows the selection and keeps it off
the edges. The pointer wheel scrolls the section under it.

## Acceptance criteria
- [x] Selection distinct from focus, visually and behaviourally
- [x] Scrolls to keep the selection visible with margin
- [x] Wheel scrolls the section under the pointer

One criterion this item started with was moved rather than met:

- Sections shrink proportionally before any of them is cut off → 0050

It was ticked here by mistake in the first pass and caught in review. The nav is
one flat list with one offset; there is no per-section height allocation
anywhere, so the last section is simply cut. Recorded rather than quietly
unticked — a ticked box that was never built is worse than an open one.
- [x] A scrollable section shows that it has more
