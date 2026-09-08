---
id: 106
title: The rail hides what is owed before it hides the clock
type: bug
status: backlog
milestone: v0.5
labels:
- rail
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: s
area: chrome
---

## What happens

On a narrow terminal the rail keeps the time and drops the count of agents
waiting for a human.

The dim run — note, scroll depth, space count, clock — is measured and placed
first, and takes whatever room it wants. The counts are then fitted in to its
left and `break` when they will not fit. Two further things compound it: the
loop draws `done` before `blocked`, and it breaks rather than continues, so
finished work can crowd out the one state that is asking for a person.

At 40 columns, from the code as it stands:

    detach_x = 25, room = 16
    right = "2 spaces  ·  1…"  ->  right_x = 8
    done "+ 2" fits             ->  right_x = 2
    blocked needs 6, 2 < 6      ->  break

## What should happen

`blocked` is the only state waiting on a human. It is the last thing the rail
gives up, not the first. Nothing is drawn before it and nothing crowds it out.

Order within the attention group is blocked, then done, then any mode. Between
groups: attention outranks the clock, the space count, the chips and the
identity, and is dropped only when there is no room for anything at all.

## Reproduction

1. Put a session in a state where an agent is blocked.
2. Narrow the terminal to 40 columns.
3. The rail shows the clock. It does not show `! 1`.

## Notes

Found while writing the design note for the rail; it is the one finding in it
that is a defect rather than a judgement, and it is separated out so it can be
fixed on its own without waiting for the rest.

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0
