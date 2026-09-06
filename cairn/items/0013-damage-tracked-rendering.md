---
id: 13
title: Damage-tracked rendering
type: chore
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: ui
---

## Problem
Every event redraws the whole frame. Bursts are coalesced so this is cheap
today, but `Ev::Output` already carries the pane id and nothing reads it.

## Acceptance criteria
- [ ] Only panes whose grid changed are re-blitted
- [ ] The chrome redraws only when the state it shows changed
- [ ] Measurably fewer bytes written per second under a noisy pane
