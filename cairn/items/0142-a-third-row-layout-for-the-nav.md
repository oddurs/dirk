---
id: 142
title: A third row layout for the nav
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: nav
effort: s
---

## Problem
`rows = "tall" | "short"` is two arrangements, and the gap between them is
large: tall gives a workspace a second line for its branch, short drops it
entirely. There is no arrangement that keeps the branch and loses the age, or
keeps the age on one line.

## Proposal
A third named layout, not a composition mechanism. herdr lets you assemble rows
from token arrays, which is more power than this needs and invites arrangements
whose columns do not line up — and the nav's column arithmetic being exact is
what keeps it readable.

Pick the third arrangement worth having, name it, and document all three
together so the choice is a choice rather than a search.

## Acceptance criteria
- [ ] A third value for `rows`, named for what it shows
- [ ] Column arithmetic exact in all three, at every sidebar width
- [ ] All three documented in one table
- [ ] No token composition mechanism
