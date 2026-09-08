---
id: 46
title: Startup and redraw budget
type: chore
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: perf
effort: m
---

## Problem
dirk feels fast and nobody has measured it. 'Superfast' was a requirement, and a
requirement with no number is a preference.

## Proposal
Budgets, checked in CI: time from exec to first frame, redraw latency under a
pane producing continuous output, and idle CPU with ten spaces open — which
should be indistinguishable from zero, because the loop blocks on a receive.

## Acceptance criteria
- [ ] Benchmarks for startup, redraw under load, and idle cost
- [ ] Numbers in the manual, and a CI failure when they regress
- [ ] Idle CPU measured with the dashboard layout open, which is the worst case
