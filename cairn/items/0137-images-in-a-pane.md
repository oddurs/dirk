---
id: 137
title: Images in a pane
type: feature
status: done
milestone: v0.8
created: 2026-09-07
updated: 2026-09-08
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
- [x] Kitty graphics parsed per pane and re-emitted for the focused layout
- [x] Images scroll with their text and survive resize and reflow
- [x] Clipped correctly at split boundaries
- [x] Nothing is emitted when it is turned off; detection is not needed — see below
- [x] An oversized or malformed image is dropped without affecting the others
- [x] `[terminal] graphics = false`

## 2026-09-08

vt100 discards a graphics APC without a word: no callback, no bytes, nothing
left in the grid. So the escape is taken out of the pane's stream before the
parser sees it, which also means a partial one is held rather than passed on —
half an APC reaching the terminal below is an escape it waits for the end of.

The anchor is the part that makes this work. A placement records the line of the
pane's *whole history* it sits on, not a row of the screen, so it moves with its
text. vt100 does not say how much has scrolled off, but it clamps
`set_scrollback` to what exists — so asking for more than there could be and
reading back gives the total, and the view is put straight back under the same
lock.

Reflow is the honest limit. A resize rewraps lines, so the line an image is
anchored to is not the line it was; what happens in practice is that the program
redraws on SIGWINCH and re-places, which is what a full-screen program does
anyway. Placements older than the scrollback are forgotten, and an image nothing
points at is dropped with them.

No capability detection. A terminal that cannot draw an image ignores the escape
— that is what the protocol is for — so querying would be a round-trip to learn
something whose wrong answer costs nothing. What it would cost is a reply
arriving in the client's *input*, which is why everything dirk sends carries
`q=2`.

A frame that has nothing new to say about images says nothing: taking the
placements off and putting them back is what a repaint is, and doing that every
tick makes a still picture flicker.

And a flaw in the smoke harness this turned up: the isolation it appends is a
`[terminal]` section, so a test with its own was a table defined twice — TOML
refuses the whole file and the test runs on the defaults in silence, which looks
exactly like the setting not working. It is folded in now rather than appended.
