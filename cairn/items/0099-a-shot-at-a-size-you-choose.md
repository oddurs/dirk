---
id: 99
title: A shot at a size you choose
type: chore
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p3
effort: s
area: web
---

## Problem

`examples/shot.rs` renders at 26x92 and nowhere else. Anything about how the
chrome behaves as the terminal narrows — which is most of what the rail does —
cannot be looked at without editing the example.

## Proposal

`DIRK_SHOT_SIZE=26x40`, defaulting to what it renders today.

Two things follow from it. `make shots` can produce more than one render for the
website, which currently shows a single width of a program whose whole layout is
width-dependent. And the ladder in 0097 becomes something a person can check by
looking rather than only by reading a test.

## Acceptance criteria

- [x] `DIRK_SHOT_SIZE=RxC` changes the size; absent, nothing changes
- [x] A bad value fails with a message rather than a panic backtrace

## Note

Done, in the branch that filed these items — it is how the design note's renders
were captured. Recorded so the change has an item behind it, as everything here
does.

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0

## 2026-09-07

A floor of 4x20 came out of doing it. Below that the shot is not a small
screen, it is a crash: the rail refuses to draw under 34 columns and the layout
has nothing to divide. Failing at the argument is better than failing in the
renderer, where the message would be about a buffer.
