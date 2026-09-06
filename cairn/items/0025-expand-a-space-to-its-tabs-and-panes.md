---
id: 25
title: Expand a space to its tabs and panes
type: feature
status: backlog
milestone: v0.2
depends_on:
- 41
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: nav
effort: m
---

## Problem
A space collapses to one row, which is right for the common case and wrong for
the dashboard: five panes behind a single line with no way to see or reach them.

## Proposal
An expander on the identity line. Expanded, a space shows its tabs, and each tab
its panes, with the pane title from the border:

    ▾ · 9 OddOS
       └ Overview                                        5 panes
          ├ brief
          ├ ptop
          └ attention

Collapsed by default, so the tree stays the height of the space list until asked.

## Acceptance criteria
- [ ] Expander toggles on click and on a key
- [ ] Expansion is remembered per space for the session
- [ ] A pane row focuses that pane directly
- [ ] A space with one pane and one tab does not draw a subtree at all
