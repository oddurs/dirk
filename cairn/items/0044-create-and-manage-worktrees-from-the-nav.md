---
id: 44
title: Create and manage worktrees from the nav
type: feature
status: done
milestone: v0.5
depends_on:
- 10
created: 2026-09-06
updated: 2026-09-07
priority: p1
area: git
effort: m
---

## Problem
Running several agents on one repository means several worktrees, and creating
one is currently: leave dirk, git worktree add, come back, open the project.
herdr makes this one action, and it is a large part of why parallel agent work
is practical there at all.

## Acceptance criteria
- [ ] Create a worktree and a space for it in one action, from the project row
- [ ] The new space opens on the new branch, named from it
- [ ] Remove a worktree, refusing when it has uncommitted changes unless forced
- [ ] Worktrees are visibly distinct from ordinary checkouts in the nav
- [ ] Listing worktrees does not require leaving dirk

## 2026-09-07

From reading herdr (see doc/parity-herdr.md), two parts worth folding in here
rather than filing separately: `[worktrees] directory` should name the checkout
root, so worktrees land somewhere chosen rather than beside the repository by
default; and closing a checkout that has open worktree spaces under it should
need explicit intent, because closing one row and losing five is not a thing
anybody meant. Drawing them under the parent is `0094`.
