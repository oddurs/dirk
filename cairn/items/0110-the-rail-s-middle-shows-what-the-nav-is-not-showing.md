---
id: 110
title: The rail's middle shows what the nav is not showing
type: feature
status: done
milestone: v0.5
assignee: oddurs
labels:
- rail
depends_on:
- 107
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: chrome
---

## Problem

The chips are a second copy of the sidebar that loses to the first. No branch,
no state, no age, and elided to fourteen columns — so `Building the mux core`
and `Building the release` both read `Building the …`. They are the first
thing dropped when the bar runs out of room, which is correct and also tells you
they are the least valuable thing on it. They are given the most room.

But they are not simply redundant. `b` hides the nav, and then they are the only
navigation there is. They are paid for in every state and useful in one.

## Decision

**One rule, for both cases: the middle of the rail shows what the nav is not
showing.**

This is not the bar changing meaning between modes. The slot's job is constant —
*as much of the session as you cannot already see* — and the content follows
from what is on screen beside it.

**Nav visible.** The list is right there, so the slot shows the one thing the
list does not: exactly where you are, down to the pane.

    dirk ▸ Building the mux core ▸ 2/3

**Nav hidden.** Nothing else is showing the session, so the slot becomes the
list.

    ▊1 mux core  ▏2 vt100 grid  ▏3 shot

This also closes the last of the four jobs the rail claims and does not do: the
pane you are actually typing into has never been named anywhere.

## Proposal

Elision in the breadcrumb keeps the end of the intent rather than the start —
see the separate item on elision, which is the same bug in the nav.

The chips keep their numbering, their hit rects and the filled-bar/thin-bar
distinction; nothing about them changes except when they are there.

## Acceptance criteria

- [x] With the nav visible, the rail names the project, the focused intent and
      the pane as `n/m`
- [x] Pressing `b` turns the middle into chips, and pressing it again turns it
      back
- [x] The chips remain clickable and keep their numbers
- [x] Neither form ever overruns the attention group

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0

## 2026-09-07

The rule holds and the breadcrumb needed one thing the design note did not
have.

Three fresh workspaces in one project are all labelled after the project, so
`dirk ▸ dirk` said the same thing in every one of them and two clients focused
on different workspaces drew identical bars. A session test caught it, which is
the test that exists to prove each client looks where it is looking.

The breadcrumb now carries the number the nav reads by and the chips jump to —
`dirk ▸ 2 Reading the vt100 grid ▸ 2/2` — so the one numbering runs through the
nav, the chips and the crumb. Without it the bar could not answer the question
it exists to answer, which is which of them you are in.
