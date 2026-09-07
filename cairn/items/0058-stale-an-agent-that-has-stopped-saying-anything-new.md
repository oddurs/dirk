---
id: 58
title: 'Stale: an agent that has stopped saying anything new'
type: feature
status: done
milestone: v0.3.1
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: naming
effort: s
---

## Problem
An agent that has revised its title once in an hour is describing work that
finished, or is stuck. namesync marks it `stale` after several state
transitions with no new intent, and deliberately never acts on it — it is a
signal for a human, not a trigger.

## Acceptance criteria
- [x] Counted in state transitions, not in wall-clock time
- [x] The `stale` token, and a mark in the nav
- [x] Nothing is renamed, skipped or notified because of it
- [x] Configurable, including off
