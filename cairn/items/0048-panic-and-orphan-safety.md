---
id: 48
title: Panic and orphan safety
type: chore
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: server
effort: m
---

## Problem
A panic restores the terminal through a hook, which is the visible half. The
other half is processes: panes are killed by Drop, and Drop does not run for
every exit path. A daemon makes this worse, since orphans then outlive the
client that could have reaped them.

## Acceptance criteria
- [ ] Every exit path leaves no orphaned child, including panic and signals
- [ ] SIGTERM and SIGHUP shut down cleanly
- [ ] The terminal is restored from every exit path, tested rather than assumed
- [ ] A daemon with no clients and no panes exits instead of lingering
