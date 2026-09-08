---
id: 107
title: An empty middle, and one place to look for what is owed
type: feature
status: done
milestone: v0.5
assignee: oddurs
labels:
- rail
depends_on:
- 106
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: chrome
---

## Problem

`rail.rs` makes the right argument and then undoes it. The counts appear only
when they are not zero, because — its own words — *"an empty middle is the
fastest possible way to say that nothing needs you, and a pair of zeroes is not
information."* And then a permanent `2 spaces` sits in that middle, so there is
never an empty middle to read.

The count is also the one element in the rail that reports something already on
screen: the sidebar lists the spaces three lines up.

Where the counts appear is not fixed either. They are placed leftward from
whatever the dim run happened to measure, so their column moves with the width
of the clock and the length of the note. A thing that is meant to catch your eye
should be somewhere your eye already is.

## Proposal

Drop `{n} spaces` entirely.

Anchor the attention group hard against the exits, so it occupies the same
columns whatever else is on the bar. What is owed is then always in one place,
and the absence of anything there is a signal you can read without looking.

The clock keeps its place to the right of attention and is the first thing
dropped under pressure.

## Acceptance criteria

- [x] Nothing is drawn in the middle of the rail when nothing is owed
- [ ] The attention group sits at the same offset from the right edge at every
      width and in every state
- [x] The space count is gone, from the rail and from the tests that assert it

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0

## 2026-09-07

Two of three. The space count is gone from the rail and from the two tests
that waited on it, and the middle is empty when nothing is owed.

The third is not true as written and could not be. What is owed is anchored
against the exits — nothing is ever laid out between them — but the exits
themselves shrink to marks further down the ladder, so a *fixed* offset from
the right edge is only available if attention moves inside them. The invariant
that holds, and is tested, is the one that matters: the count is always found
immediately left of the way out.

One thing changed from the design note while doing it. The note drew what is
owed to the left of the clock. That puts seven columns of clock between the
count and the bar\x27s end, and they vanish when the clock is given up — so the
count moved. It sits between the clock and the exits instead.
