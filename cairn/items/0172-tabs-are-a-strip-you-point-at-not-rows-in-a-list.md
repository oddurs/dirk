---
id: 172
title: Tabs are a strip you point at, not rows in a list
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: chrome
depends_on:
- 156
- 157
- 170
---

## Problem
Tabs exist and there is nowhere to point at them. They are rows in the nav
under an expanded space, which is where you go to see the shape of a session --
not where you go while working in one. The rail shows chips for spaces, not
tabs. So switching tabs is `,` and `.`, making one is `c`, closing one is `&`,
and none of it is pointable.

Tabs are the one part of dirk whose interface everybody already knows from
somewhere else, and it is the part with no pointer surface at all.

## Proposal
A strip across the top of the space's panes, when the space has more than one
tab -- and a `+` when it has one, which is how you get a second without knowing
that `c` exists.

    │ mux core │ tests │ + │

Click to switch. Middle-click or the `✕` to close, under the live-work rule.
Double-click to rename. Drag to reorder, which is nearly free once drop targets
exist and is the second thing anybody tries.

One tab draws no strip and one `+`, because a strip naming "the only
arrangement" is the same row the nav already decided not to draw for the same
reason.

The strip costs a row from the panes, which is why it is not there when there
is one tab, and why this is worth doing after the pane strip rather than before
-- a space with one tab and one pane should still be a pane and not a frame.

## Acceptance criteria
- [ ] A strip when there is more than one tab; a `+` alone when there is one
- [ ] Click switches, and the strip says which one is live
- [ ] Close from the strip, under the same live-work rule as everything else
- [ ] Double-click renames
- [ ] Tabs reorder by dragging
- [ ] One tab draws no strip
- [ ] Names elide the way labels elide, not the way identifiers do
