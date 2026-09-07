---
id: 28
title: Age, right-aligned
type: feature
status: done
milestone: v0.2
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: nav
effort: s
---

## Problem
'How long has this been sitting there' is most of triage, and nothing shows it.

## Proposal
A coarse age on the right of the identity line: now, 10m, 2h, 1d. Coarse on
purpose — a per-second clock would redraw constantly for a value nobody reads
that precisely.

For a space, age is time since its pane last produced output. For an agent row,
time since it entered its current state, which is the number that matters when
the state is blocked.

## Acceptance criteria
- [x] Bucketed to now / minutes / hours / days
- [x] Right-aligned and never collides with an elided title
- [x] Effectively recomputed on the tick — the string is built during
      drawing, but dirk only redraws on an event, and the tick is the one
      that makes an age change
