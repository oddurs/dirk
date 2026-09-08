---
id: 160
title: The pointer leaves a trace
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
- 158
---

## Problem
Pointing at something in dirk does nothing until you click it. There is no
hover state anywhere -- not on a space row, not on a board, not on the two ways
out in the corner, not on the footers that are already clickable.

Without it a mouse-first interface cannot work, for a reason that is not about
polish: **you cannot find out what is clickable except by clicking it.** Every
target in dirk is discovered by guessing and being right. That is tolerable in
a keyboard interface where the footers tell you the keys, and it is the whole
problem in one where pointing is the primary way through.

The events are already arriving -- see the item about what they cost -- so this
is a matter of spending something already paid for.

## Proposal
The spot under the pointer is drawn differently. Not a border and not a colour
invented for the purpose: the theme already separates `selected` from `text`
from `dim`, and hover is a fourth weight in that family, weaker than selection
because the selection is where the keyboard is and the pointer is only passing
through.

Two things it must not do:

- **Move anything.** A hover that changes a width or inserts a mark reflows the
  row under the pointer, and the thing you were about to click walks away.
- **Follow focus.** Hover is where the pointer is; selection is where the
  keyboard is. They are different and drawing them the same way makes the
  column lie about which one moves when you type.

A pane is not hovered. The pointer over a pane is over the program's business,
and lighting it up would be dirk drawing on content that is not its.

## Acceptance criteria
- [ ] The spot under the pointer is drawn distinctly, in every list that has spots
- [ ] Hover and selection are visually distinct, and neither implies the other
- [ ] Hovering never changes any width, and a test says so
- [ ] Panes are not hovered; the chrome is
- [ ] The pointer leaving the window or the nav clears it, rather than stranding it
- [ ] One redraw per change of hovered spot, not per cell crossed
