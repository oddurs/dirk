---
id: 33
title: Notify when an agent blocks or finishes
type: feature
status: done
milestone: v0.3
assignee: oddurs
depends_on:
- 31
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: agents
effort: m
---

## Problem
The point of running several agents is not watching them. Attention in the nav
only helps when you are looking at dirk.

## Proposal
A system notification on the transitions worth interrupting for: entering
blocked, and finishing unseen work. Not on every state change, and never for a
space that is currently focused — you can already see it.

Rate limited per workspace, because an agent that blocks, unblocks and blocks
again inside a minute is one interruption.

## Acceptance criteria
- [x] Notifies on blocked and on done, not on working or idle
- [x] Silent for the focused space
- [x] Rate limited per workspace
- [x] Configurable, including off

One criterion was moved rather than met:

- Clicking the notification focuses that space → 0053

macOS delivers notifications through `osascript`, which has no way to hand a
click back to a program that is not an app bundle. Doing it properly means
either shipping one or using a helper, and that is a packaging decision rather
than a line of code. Recorded rather than quietly unticked.
