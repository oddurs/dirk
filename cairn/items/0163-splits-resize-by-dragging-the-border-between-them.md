---
id: 163
title: Splits resize by dragging the border between them
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: layout
depends_on:
- 156
- 159
---

## Problem
The sidebar divider drags and nothing else does. A split cannot be resized by
pointing at the boundary between two panes, which is how every other program
with panes has worked for thirty years -- and it is the single gesture people
try first.

This is the pointer half of `resizing a split from the keyboard`, which does
the other half as a mode after the prefix. Both are wanted and neither replaces
the other: the mode is for precision and for people who never reach for a
pointer, and the drag is for the ordinary case of "that one wants to be wider".

It also needs something to grab. Borders are drawn for splits today and not for
a lone pane, which `pane borders, and turning the mouse off` addresses -- the
two should land together or this has an invisible target.

## Proposal
A press on a border grabs it; the drag moves that boundary; release drops it.
The grab is the one from the pointer item, so the drag survives the pointer
leaving the border, which it will immediately.

The boundary that moves is the one actually there. Nested splits mean the
pointer is over exactly one boundary, and moving the outermost because it was
easier to find is the failure the keyboard item names too.

Minimum sizes hold. A pane cannot be dragged to nothing, and the drag stops at
the floor rather than the pane vanishing and the layout rearranging under the
pointer.

Double-click on a border equalises the two sides, which is the other gesture
people already have in their hands.

## Acceptance criteria
- [ ] A press on a border grabs it; the drag moves it; release drops it
- [ ] The drag continues while the pointer is off the border, and off the pane
- [ ] Nested splits move the boundary under the pointer, not the outermost
- [ ] Minimum sizes hold; the drag stops rather than the pane disappearing
- [ ] Double-click on a border equalises
- [ ] A pane that wants the mouse does not swallow a press on the border beside it
