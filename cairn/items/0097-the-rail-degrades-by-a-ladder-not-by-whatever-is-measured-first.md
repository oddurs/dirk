---
id: 97
title: The rail degrades by a ladder, not by whatever is measured first
type: chore
status: backlog
milestone: v0.5
labels:
- rail
depends_on:
- 107
- 109
- 110
- 105
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: m
area: chrome
---

## Problem

What the rail gives up as the terminal narrows is decided by the order things
happen to be measured in, not by what they are worth. That is how it ends up
keeping the clock and dropping the count of agents waiting for a human (0106),
and how at 34 columns it produces `2 spaces…` — half a word nobody needed.

## Proposal

An ordered list, applied cumulatively, each step giving up strictly less than
the step before it. The bar then shrinks monotonically and can never trade
something needed for something not.

    1  clock            ambient; every terminal has one
    2  pane counter     n/m is a nicety
    3  project          the nav still says it
    4  session name     rarely more than one
    5  exit words       detach and quit become ⏏ and ✕
    6  context          the intent goes; the nav has it
    7  host             late: a hazard, not decoration
    8  the user's name  the mark alone
    —  attention        never

Attention is not on the list. If there is no room for it, everything else goes
first.

Below the floor the rail draws the mark, what is owed, and one way out. It does
not draw a shortened version of something else.

## Acceptance criteria

- [ ] The rail is laid out by one ordered list of steps, in one place
- [ ] Every width from 20 columns upward produces a row of exactly that width
      with no element overlapping another
- [ ] Attention survives every step that anything survives
- [ ] A test walks the widths and asserts the ladder is monotonic — nothing that
      was dropped comes back as the terminal narrows further

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0
