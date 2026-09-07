---
id: 85
title: Repository settings a public project should have
type: chore
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: packaging
---

## Problem

The repository is public and configured as if it were not. Anything can be
pushed straight to main; merged branches accumulate rather than being deleted;
auto-merge is off, so the last step of an automated workflow is a human
pressing a button.

## Proposal

- A ruleset on `main`: a pull request, and CI green before it can merge. The
  admin keeps a bypass, so nothing can wedge.
- Squash merges only, matching the history that is already there.
- Delete the branch on merge, and allow auto-merge, which is what makes
  `git work ship` the last command anyone has to run.

## Acceptance criteria

- [x] main cannot be pushed to directly
- [x] A pull request merges itself once style, linux, macos and msrv are green
- [x] Branches are deleted when they merge

## 2026-09-07

The ruleset is active on main: a pull request, and style, linux, macos, msrv
and docs green before it can merge. Squash only, delete on merge, auto-merge
on. The admin keeps a bypass so nothing can wedge.

The second criterion is proven by the pull request carrying this item, which
merges itself when its own checks pass.
