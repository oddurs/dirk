---
id: 133
title: Resizing a split from the keyboard
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: mux
effort: m
---

## Problem
The sidebar divider drags and the splits do not, from either input. A layout you
cannot adjust without reaching for a mouse is one people leave wrong.

## Proposal
A resize mode after the prefix: `hjkl` and the arrows move the boundary the
focused pane sits against, Escape and Enter leave. A mode rather than a chord
because resizing is never one keystroke, and holding a modifier through six of
them is worse than pressing one key first.

The rail says which mode you are in, as it does for the nav.

## Acceptance criteria
- [x] Prefix then `R` enters; Escape and Enter leave — `r` was taken, see below
- [x] `hjkl` and arrows move the relevant boundary
- [x] A count prefix, so `10l` is one gesture
- [x] Nested splits resize the boundary that is actually there, not the outermost
- [x] Minimum sizes hold; a pane cannot be resized to nothing
- [x] The rail names the mode

**Related.** This is the keyboard half. `splits resize by dragging the border
between them` is the pointer half, and both are wanted: a mode for precision
and for anybody who never reaches for a pointer, a drag for "that one wants to
be wider".

## 2026-09-08

`R`, not `r`. `r` restarts a stopped pane and says so on that pane's own rule —
a promise already made on the screen, and not one to take back for a mode.

Shares are rewritten in cells rather than nudged as weights. `Constraint::Fill`
is a ratio, so nudging the weights makes a press mean "a third of the space" on
a two-pane split and something else on a three-pane one; reading the current
sizes and writing them back as weights makes a press a column, and leaves every
other child exactly where it was.

The boundary moves, rather than the focused pane growing. From the pane on the
right of a pair, `l` moves the edge right and makes that pane narrower. tmux
agrees, and the alternative has `l` and `h` swapping meaning depending on which
pane you are in.

The innermost split running the right way is the one that moves, found by
recursing before handling the level you are on — otherwise a column inside a row
resizes the row's edge, which is a boundary somewhere else on the screen and the
version of this that people give up on.
