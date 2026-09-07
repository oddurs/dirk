---
id: 32
title: Attention counts in the rail
type: feature
status: done
milestone: v0.3
assignee: oddurs
depends_on:
- 31
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: chrome
effort: s
---

## Problem
The rail shows a space count, which never changes and tells you nothing. The
number that matters is how many agents are waiting.

## Proposal
Counts, and only when they are non-zero:

    ◆ dirk   ! 2  + 3            9 spaces  ·  19:10

An empty middle is the fastest possible way to say nothing needs you.

## Acceptance criteria
- [x] Blocked and done counts, hidden at zero
- [x] Clicking a count jumps to the oldest workspace in that state
- [x] Recomputed on state change, not on a timer
