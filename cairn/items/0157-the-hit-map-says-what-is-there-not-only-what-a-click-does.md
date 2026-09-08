---
id: 157
title: The hit map says what is there, not only what a click does
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: m
area: chrome
depends_on:
- 156
---

## Problem
`Target` is what a click *does*: `Workspace { p, w }`, `SpaceFold { p, w }`,
`Layout(i)`, `Detach`. Exactly right for clicking, and not enough for anything
else a pointer wants to do.

Three things need to know what is *there* rather than what would happen:

- A context menu offers the actions that apply to the thing under the pointer.
  It cannot ask what a click would do, because the answer is one action and a
  menu wants the set.
- Hover has to say what a thing is. `SpaceFold { p, w }` names an effect; there
  is no name anywhere for the row it is on.
- A drag has a source and a drop has a target, and neither is an action. A pane
  dragged onto a split is not "the action that split would perform".

## Proposal
The hit map keeps a `Spot`: the rectangle, the target as now, and what the
thing *is* -- a space, a checkout, a tab, a pane, a board, a heading, the
divider, a scrollbar. The target stays optional, because some spots are worth
pointing at and dropping onto without being worth clicking.

Two rules do not move, because both are why the hit map can be trusted: it is
built while drawing rather than modelled separately, and the last spot
registered wins, so a widget drawn on a row beats the row under it.

## Acceptance criteria
- [ ] A spot carries what it is, not only what clicking it would do
- [ ] Objects that can be pointed at but not clicked can be registered
- [ ] Last-registered still wins, and a test says so
- [ ] Nothing builds a second pass over the layout to answer "what is here"
- [ ] Every existing target still works, unchanged, through the new shape
