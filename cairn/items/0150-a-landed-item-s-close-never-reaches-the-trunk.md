---
id: 150
title: A landed item's close never reaches the trunk
type: bug
status: done
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: packaging
---

## What happens

## What should happen

## Reproduction

1.

## Problem
`git work land` closes the item and then removes the worktree.  The close is a
file edit in whichever checkout the command was run from -- the trunk one, by
then, because the branch has already merged -- and nothing commits it.  So it
sits there uncommitted until somebody resets, and the backlog says `review` or
`backlog` for work that shipped weeks ago.

Five items landed and none of them recorded it.  The failure is silent in the
worst way: `land` prints `closed 0092` and is telling the truth about what it
did to the file.

## Shape
The close has to become a commit on the trunk, which means `land` either
commits and pushes it, or does the close *before* `ship` pushes -- as part of
the branch, the way `ship` already writes `status: review`.

The second is the smaller change and the better record: the commit that closes
the item is the commit that did the work.  `land` then has nothing to close and
only has to check that the item is done, and say so if it is not.

## Acceptance criteria
- [x] An item that landed reads `done` in the trunk, without anybody editing it
- [x] The close is in the branch's own history, not a loose edit afterwards
- [x] `land` says something if it is asked to land work whose item is still open
- [x] Landing twice, or landing a branch with no item, is not an error
