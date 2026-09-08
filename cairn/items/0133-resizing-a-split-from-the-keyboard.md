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
- [ ] Prefix then `r` enters; Escape and Enter leave
- [ ] `hjkl` and arrows move the relevant boundary
- [ ] A count prefix, so `10l` is one gesture
- [ ] Nested splits resize the boundary that is actually there, not the outermost
- [ ] Minimum sizes hold; a pane cannot be resized to nothing
- [ ] The rail names the mode

**Related.** This is the keyboard half. `splits resize by dragging the border
between them` is the pointer half, and both are wanted: a mode for precision
and for anybody who never reaches for a pointer, a drag for "that one wants to
be wider".
