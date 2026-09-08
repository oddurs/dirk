---
id: 137
title: Images in a pane
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: mux
effort: xl
---

## Problem
dirk emulates the terminal in each pane, so a program that draws an image draws
it to dirk and stops there. A plot, a diagram, a screenshot an agent just took:
all of them work in the terminal and stop working the moment they are inside
dirk. This is the largest single thing that makes a dirk pane not a real
terminal.

## Proposal
Parse the kitty graphics protocol in the pane's terminal, hold the image against
the cells it occupies, and re-emit it to the outer terminal at the right place
when that pane is drawn — including through the outer terminal's own
constraints, and not at all when the outer terminal cannot do it.

The hard parts are the ones that show: images that scroll with their text,
images that survive a resize and a reflow, an image partly hidden behind a split
boundary, and an oversized image that must not be able to prevent a small one
from appearing.

On by default where the outer terminal supports it, and `[terminal] graphics =
false` to turn it off — this is enough new parsing that somebody will want the
switch.

## Acceptance criteria
- [ ] Kitty graphics parsed per pane and re-emitted for the focused layout
- [ ] Images scroll with their text and survive resize and reflow
- [ ] Clipped correctly at split boundaries
- [ ] Capability detected; nothing emitted to a terminal that cannot draw it
- [ ] An oversized or malformed image is dropped without affecting the others
- [ ] `[terminal] graphics = false`
