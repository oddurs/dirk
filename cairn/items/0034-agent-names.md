---
id: 34
title: Agent names
type: feature
status: backlog
milestone: v0.3
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
- [ ] Names match [a-z][a-z0-9_-]{0,31} and are unique among live agents
- [ ] A name follows its agent and is cleared when the agent exits
- [ ] Naming suggests one from the intent, with a suffix on collision
- [ ] Both name and pane id are accepted wherever an agent is addressed
