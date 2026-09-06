---
id: 33
title: Notify when an agent blocks or finishes
type: feature
status: backlog
milestone: v0.3
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
- [ ] Notifies on blocked and on done, not on working or idle
- [ ] Silent for the focused space
- [ ] Rate limited per workspace
- [ ] Configurable, including off
- [ ] Clicking the notification focuses that space, where the platform allows it
