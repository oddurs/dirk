---
id: 91
title: A project is a repository, not a directory
type: feature
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: nav
effort: l
---

## Problem
dirk keys a project by its path, and a git worktree is a different path.  So a
worktree opens as its own top-level project sitting beside the repository it
belongs to, wearing a directory name nobody chose -- `dirk-feat-packaging`, or
worse `worktree-green-meadow-52eb`.

The sidebar is the product, and this is the row that most often looks like
sediment rather than like work.

## What a project is
The repository, found by its common git dir -- what `git rev-parse
--git-common-dir` answers, which is the same path for a repository and for
every worktree of it.  A project then holds *checkouts*, and a space belongs to
one of them.

That is one modelling change and everything else in this group falls out of it:
the header can be the repository's name, the branch belongs to the checkout
rather than to the project, and a worktree has somewhere to be drawn.

## Depth is one, and stays one
A worktree of a worktree is still a worktree of the same repository, so the
tree bottoms out immediately.  There is no recursion to design and none should
be written: the nav is a fixed three levels -- project, space, and the panes
under a space -- with worktree spaces one indent deeper than the rest.

## Acceptance criteria
- [ ] Checkouts of one repository group under one header
- [ ] The header is the repository's name, not the directory a checkout is in
- [ ] Opening a worktree of an open project does not make a second project
- [ ] Closing the last space in a worktree does not close the project
- [ ] Persistence restores the grouping rather than a flat list of directories
