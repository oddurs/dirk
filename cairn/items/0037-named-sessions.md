---
id: 37
title: Named sessions
type: feature
status: done
milestone: v0.4
assignee: oddurs
depends_on:
- 7
created: 2026-09-06
updated: 2026-09-07
priority: p2
area: server
effort: m
---

## Problem
One daemon and one session is enough until it is not: a long experiment you want
to leave running, or a separate set of spaces for a different context.

## Acceptance criteria
- [x] dirk --session <name> creates or attaches
- [x] dirk session list shows what exists, and whether anything is attached
- [x] Sessions are independent: closing one does not touch another
- [x] A session with no client keeps running
