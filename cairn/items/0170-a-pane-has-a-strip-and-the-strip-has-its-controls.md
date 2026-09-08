---
id: 170
title: A pane has a strip, and the strip has its controls
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: m
area: chrome
depends_on:
- 156
- 157
---

## Problem
A pane with a label reserves one row above it for a rule and that name. A pane
without one gets nothing. So the only pane chrome dirk has is a caption, it
appears on the minority of panes that were named by a layout, and it holds
nothing you can press.

Meanwhile the pane is the thing people do the most to: split it, zoom it, close
it, restart it when it dies. All of that is keyboard-only.

Every other program with panes puts those where the pane is. dirk has the row
already reserved and spends it on a word.

## Proposal
The strip becomes the pane's own controls: what it is on the left, what you can
do to it on the right.

    ─ ptop ─────────────────────────────  ⤢  ✕

Zoom and close, because those are the two people reach for; split from the
menu, because two more marks on every pane is a toolbar and this is a terminal.
A dead pane's strip offers restart, in the place close would be, because a
stopped pane is the one case where the obvious press is not "get rid of it".

It has to appear on every pane, not only labelled ones, which is the same
change `pane borders, and turning the mouse off` wants for its own reasons --
these should land together. A pane with no label shows what is running in it,
which is what the nav already computes for pane rows and should not compute
twice.

Two things to hold to. It costs the row it already costs and no more: panes are
small and a second row of chrome on a three-line pane is chrome with a pane
attached. And the marks only appear when the pane is big enough for them --
under about twenty columns the name is worth more than the controls.

## Acceptance criteria
- [ ] Every pane has a strip, not only labelled ones
- [ ] Zoom and close are on it and work
- [ ] A dead pane offers restart where close would be
- [ ] An unlabelled pane's strip says what is running, computed once
- [ ] It costs one row, the row already reserved
- [ ] Controls are dropped before the name when the pane is narrow
- [ ] The strip is not part of the terminal: nothing the program draws lands on it
