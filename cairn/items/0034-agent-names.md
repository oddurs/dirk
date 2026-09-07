---
id: 34
title: Agent names
type: feature
status: done
milestone: v0.3
assignee: oddurs
depends_on:
- 30
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: agents
effort: s
---

## Problem
Addressing an agent by pane id is fine for a program and poor for a person. A
name that follows the agent and is unique among live agents is what makes
`dirk agent send-keys reviewer` possible.

## Acceptance criteria
- [x] Names match [a-z][a-z0-9_-]{0,31} and are unique among live agents
- [x] A name follows its agent and is cleared when the agent exits
- [x] Naming suggests one from the intent, with a suffix on collision
- [ ] Both name and pane id are accepted wherever an agent is addressed —
      nothing addresses an agent yet. The names exist and are unique, which is
      what that will need; the surface is 0035 in v0.4.
