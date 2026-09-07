---
id: 85
title: Repository settings a public project should have
type: chore
status: doing
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

- [ ] main cannot be pushed to directly
- [ ] A pull request merges itself once style, linux, macos and msrv are green
- [ ] Branches are deleted when they merge
