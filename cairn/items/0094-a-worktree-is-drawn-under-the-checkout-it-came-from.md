---
id: 94
title: A worktree is drawn under the checkout it came from
type: feature
status: done
milestone: v1.0
depends_on:
- 91
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: nav
effort: m
---

## Problem
Once a project is a repository rather than a directory, its worktrees have
somewhere to be.  They are not peers of the spaces in the main checkout -- they
are the same work on another branch, which is the whole reason somebody made
one -- so they are drawn one indent deeper, with a connector saying what they
hang off.

    ▾ dirk
      ◐ 4 · try ↑6 ↓1                  ▾
        Namesync multiplexer TUI
         └ ○ 9 · worktree
             Git workflow setup for…

## One level
A worktree of a worktree is another worktree of the same repository, so the
indent never goes to three.  Writing this as a general tree would be writing a
recursion that cannot happen and then maintaining it.

## Acceptance criteria
- [x] A worktree's spaces are indented one level under the project
- [x] A connector shows what they hang off
- [x] The indent is never deeper than one, and the code cannot express deeper
- [x] The disclosure that folds a checkout is on the row it folds
      — no checkout row and so no checkout fold: the spaces hang directly
        off the project, and the only disclosures are the ones already on
        the rows they fold
