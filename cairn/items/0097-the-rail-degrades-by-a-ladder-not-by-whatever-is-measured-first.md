---
id: 97
title: The rail degrades by a ladder, not by whatever is measured first
type: chore
status: done
milestone: v0.5
assignee: oddurs
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

- [x] The rail is laid out by one ordered list of steps, in one place
- [x] Every width from 20 columns upward produces a row of exactly that width
      with no element overlapping another
- [x] Attention survives every step that anything survives
- [x] A test walks the widths and asserts the ladder is monotonic — nothing that
      was dropped comes back as the terminal narrows further

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0

## 2026-09-07

One list, in one place, and two things came out of building it.

**The middle sizes itself.** Dropping the context outright when it could have
been shortened gives up more than the step above it, which is the one thing an
ordered list is for. So everything except the middle has a size of its own and
the middle takes what is left.

**And it refuses to be illegible.** Shortening a label to `2 R…` counted as
fitting, so an early step succeeded and the steps that would have dropped the
project and given the name its room back never ran. The middle now answers
"no" below a floor and the ladder takes another step.

The floor is measured on what is drawn, not on the room. Shortening lands on a
word boundary, so ten columns of room can produce five columns of name — and
measuring the room rather than the name is how fifty columns came to show less
than forty-four did, by keeping the project and spending the difference on
nothing.

`the_rail_fits_every_width_it_is_given` walks sixteen through eighty and
asserts the row is never wider than the terminal, always says something, and
never wraps onto the row above — which for the one surface that spans the whole
screen is the failure that corrupts a pane.

## 2026-09-07

A code review found the one thing the ladder could not survive.

The status note was returned at whatever length it happened to be, whatever
room there was — so on a narrow terminal in copy mode no step could fit, every
one of them failed, and the bar fell back to drawing the mark alone. The way
out, the clock and anything owed all went, at the moment they were hardest to
guess at.

The note is cut to the room now, at the end rather than the front because it is
a sentence, and it is subject to the same step that drops the crumb. Two tests
hold it: one enters copy mode and narrows to twenty-four columns asserting a
way out is always on the bar, and the width walk asserts the same at every
width whether or not there is a note.
