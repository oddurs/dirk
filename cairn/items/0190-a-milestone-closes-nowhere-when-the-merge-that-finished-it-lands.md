---
id: 190
title: A milestone closes nowhere, when the merge that finished it lands
type: bug
status: done
milestone: v1.0
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: s
area: packaging
---

## What happens
`scripts/milestones` keeps every milestone as done as the work in it, and
cairn's hooks run it whenever anything changes. Neither of those happens on the
trunk: item statuses arrive there by merge, and a merge does not run cairn.

So a branch that finishes the last item in a milestone lands with the item
closed and the milestone still open. The next `make lint` finds a milestone
complete and open and fails -- for whoever happens to push next, who had
nothing to do with it.

v0.9 was the first. Its last item was caught by hand before it landed, which is
not a plan.

## What should happen
The branch that finishes a milestone carries the milestone's close, the same
way it already carries its own item's. `git work ship` runs `scripts/milestones`
after closing the item and commits whatever it changed.

Which is the same fix as the item one level down, for the same reason: the
trunk is protected and takes changes only through a pull request, so anything
that has to reach it has to be in a branch. There is nowhere else to put it.

Run unconditionally rather than only when an item closed: a branch that reopens
something can complete or uncomplete a milestone too, and the script is the
thing that decides which.

## Reproduction
1. Finish the last open item in a milestone, and land it.
2. `make lint` on the trunk: `milestone vX is finished and still open`.
