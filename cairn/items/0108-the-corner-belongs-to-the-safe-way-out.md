---
id: 108
title: The corner belongs to the safe way out
type: feature
status: done
milestone: v0.5
assignee: oddurs
labels:
- rail
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: chrome
---

## Problem

The bottom-right corner is the cheapest target a pointer has: you can throw the
mouse at it and it cannot overshoot. That corner is given to **quit**, which
ends every shell and every agent in the session. **detach** — the safe thing
you do several times a day — is an interior strip immediately to its left, so
overshooting it lands on quit.

The two-click arming on quit is the right instinct and it is compensating for a
layout that should not need compensating for.

## Decision

**detach takes the corner. quit sits inboard of it, behind a gap.**

The argument against is convention: a `✕` belongs bottom-right. It does not
apply here, for two reasons.

The first is that this is not window chrome. The terminal emulator already has
a close button and it is somewhere else; a second `✕` in the bottom corner of
the *content* is not a convention anyone is relying on.

The second is that this project abandoned that convention already, on purpose.
The bar spells both exits in words because *"a single ✕ cannot say which one it
is"*. Having decided the glyph could not carry the meaning, there is nothing
left to preserve by keeping it in the corner.

Quit keeps its two clicks. This is not a replacement for that; it is the thing
that should have been true before it was needed.

## Acceptance criteria

- [x] detach occupies the last cells of the rail, including the final column
- [x] quit is separated from detach by at least two cells of ground
- [x] quit still arms on the first click and ends the session on the second
- [x] The smoke test for two-click quit finds the button in its new place

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0

## 2026-09-07

Detach holds the corner, quit is inboard behind two cells of ground, and quit
still takes two clicks.

`the_corner_is_the_safe_way_out` asserts the order, the last column and the
gap, so a later change that puts them back cannot pass quietly.
