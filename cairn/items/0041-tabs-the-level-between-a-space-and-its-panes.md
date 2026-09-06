---
id: 41
title: 'Tabs: the level between a space and its panes'
type: feature
status: backlog
milestone: v0.5
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: mux
effort: l
---

## Problem
A workspace holds a flat list of panes. Real work wants a second axis — an
editor tab and a test-runner tab in one space — and the nav already draws a tab
level when a space is expanded, with nothing behind it.

## Acceptance criteria
- [ ] A space holds tabs; a tab holds a split tree of panes
- [ ] Tabs are named, by hand and by the naming policy
- [ ] The expanded nav row reflects real tabs
- [ ] Ids are workspace-qualified: w1:t1, w1:p1
