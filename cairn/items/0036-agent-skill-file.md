---
id: 36
title: Agent skill file
type: docs
status: backlog
milestone: v0.4
depends_on:
- 35
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: docs
effort: s
---

## Problem
An agent that can drive dirk still has to be told how, and a README is the wrong
shape for that.

## Proposal
`dirk --skill` prints a skill file: what dirk is, how to check it is running
inside one, the discovery commands, and the traps — that an agent name is not a
pane id, that a moved pane gets a new id, that reads do not mark seen.

## Acceptance criteria
- [ ] dirk --skill prints it, and it is generated from the real command surface
- [ ] Documents how to verify the environment before issuing any command
- [ ] Documents the id rules and the seen rule
- [ ] Checked in CI against the actual CLI, so it cannot drift
