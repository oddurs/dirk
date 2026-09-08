---
id: 188
title: A finished milestone still says it is open
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
Nine milestones are complete and every one of them still says `backlog`. v0.1
through v0.8: every item in each is done or dropped, and the milestone's own
status has never moved.

It is invisible because the roadmap draws its progress bar from the child items
-- `v0.5 ########## 29/29` -- so the page looks right. The milestone's own
status is the one field nothing reads and nothing maintains.

## What should happen
A milestone is as done as the work scheduled into it. That is a fact about the
other items, not a thing for somebody to remember, so it is worked out rather
than typed.

`git work ship` cannot do it. It closes the item a branch carried, and a
milestone has no branch: no commit finishes one, only the commit that finishes
the last thing in it. That is the same shape as the bug where a landed item's
close never reached the trunk, one level up.

cairn has hooks, which is the seam. `scripts/milestones` reads the board and
brings every milestone into line, and `after-create`, `after-change` and
`after-remove` all run it -- all three, because all three can change the
answer: the last item closing, a new one scheduled into a finished milestone,
and the last open one deleted out of it.

It reopens as well as closes. A one-way door would only move the lie: reopen an
item in a closed milestone and the milestone would be wrong in the other
direction, quietly, forever.

An empty milestone is not finished, it is empty. Closing one would mean a
milestone is done the moment it is created and undone again as soon as anything
is scheduled into it.

`--check` changes nothing and fails if anything is out of line, and `make lint`
runs it -- for the case the hook cannot see, which is somebody editing an item
by hand or a merge bringing two branches' items together.

## Reproduction
1. `cairn list --all -t milestone` -- v0.1 through v0.8 all say `backlog`.
2. Every one of them is at n/n in `ROADMAP.md`.
