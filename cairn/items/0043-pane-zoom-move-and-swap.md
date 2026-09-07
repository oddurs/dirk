---
id: 43
title: Pane zoom, move and swap
type: feature
status: doing
milestone: v0.5
depends_on:
- 11
created: 2026-09-06
updated: 2026-09-07
priority: p2
area: mux
effort: m
---

## Problem
A workspace is a split tree and the only thing you can do to it is add a leaf.
Once there are four panes in it, the one you are reading is a quarter of the
screen and there is no way to make it the whole of it without closing the other
three.

Nor is there any way to rearrange one.  A pane lands where the split that made
it put it, and a dashboard whose panels came out in the wrong order stays in
the wrong order.

## Zoom is a view, not a change
The tree is not touched.  Zoom is a flag on the workspace saying "draw only
this leaf", so leaving it puts every pane back exactly where it was and nothing
running notices anything except a resize.

That matters more than it sounds: the alternative -- closing the others and
reopening them -- is not the same operation, and the programs inside would not
survive it.

## Acceptance criteria
- [ ] One pane fills the workspace, and leaving puts the rest back untouched
- [ ] A zoomed workspace says so, since a single pane looks like a single pane
- [ ] Move the focused pane within its tree
- [ ] Swap two panes without either losing what is running in it
- [ ] The nav's pane subtree reflects the order after a move
