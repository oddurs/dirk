---
id: 36
title: Agent skill file
type: docs
status: done
milestone: v0.4
assignee: oddurs
depends_on:
- 35
created: 2026-09-06
updated: 2026-09-07
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
- [x] dirk --skill prints it, and it is generated from the real command surface
- [x] Documents how to verify the environment before issuing any command
- [x] Documents the id rules and the seen rule
- [x] Generated from the command table, so it cannot drift — and a test asserts every command appears in it
