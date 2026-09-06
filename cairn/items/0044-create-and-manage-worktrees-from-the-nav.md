---
id: 44
title: Create and manage worktrees from the nav
type: feature
status: backlog
milestone: v0.5
depends_on:
- 10
created: 2026-09-06
updated: 2026-09-06
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
