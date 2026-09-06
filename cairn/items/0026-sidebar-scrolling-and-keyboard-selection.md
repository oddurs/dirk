---
id: 26
title: Sidebar scrolling and keyboard selection
type: feature
status: planned
milestone: v0.2
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
- [ ] Selection distinct from focus, visually and behaviourally
- [ ] Scrolls to keep the selection visible with margin
- [ ] Wheel scrolls the section under the pointer
- [ ] Sections shrink proportionally before any of them is cut off
- [ ] A scrollable section shows that it has more
