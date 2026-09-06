---
id: 37
title: Named sessions
type: feature
status: backlog
milestone: v0.4
depends_on:
- 7
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: server
effort: m
---

## Problem
One daemon and one session is enough until it is not: a long experiment you want
to leave running, or a separate set of spaces for a different context.

## Acceptance criteria
- [ ] dirk --session <name> creates or attaches
- [ ] dirk session list shows what exists, and whether anything is attached
- [ ] Sessions are independent: closing one does not touch another
- [ ] A session with no client keeps running
