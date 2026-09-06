---
id: 49
title: A layout pane whose program exits should not vanish
type: bug
status: backlog
milestone: v0.2
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: layout
effort: s
---

## What happens
A layout pane whose program exits is reaped like any other pane: it is removed
and the split collapses, so the remaining panes silently grow to fill the space.

Found while looking at a dashboard whose panels were `cairn roadmap` and
`cairn board`. Both print and exit, so within a second the layout was a single
full-screen ptop and there was no indication that two panels had ever existed.

## What should happen
For a shell you opened, closing it should close the pane — that is the whole
contract. For a pane a *layout* declared, the arrangement is the thing the user
named and asked for, and it should survive its contents exiting.

The output is usually the point: `cairn board` printing a board and exiting is
a perfectly reasonable panel, and it is the panel dirk is least able to show.

## Reproduction
1. Declare a layout with a pane running `cairn board`.
2. Open it.
3. The pane is gone within a second, and the others have grown.

## Acceptance criteria
- [ ] A layout pane keeps its rectangle when its program exits
- [ ] Its last output stays on screen, dimmed, with the exit status in the rule
- [ ] A key restarts it in place
- [ ] Closing a pane by hand still closes it — this is about exiting, not closing
- [ ] A layout is only torn down when every pane has gone
