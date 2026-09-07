---
id: 82
title: A worktree per branch, and one verb-set that drives it
type: chore
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: git
---

## Problem

Every branch in this repository was made by hand, in the same checkout, one at a
time. That is fine for one person and hopeless for several agents: a coding
agent that has to `git switch` shares a working tree with whoever else is
running, so two pieces of work cannot be in flight at once without one of them
corrupting the other's build.

Worktrees solve the isolation. What is missing is everything around them — the
branch, the claim on the backlog item, the pull request, the merge, and the
cleanup afterwards — which is currently six commands and a good memory.

## Proposal

One script, `scripts/work`, wired in as `git work`, with a verb per step of the
loop:

    git work start <ID|name>   claim the item, branch from origin/main, add a worktree
    git work sync              rebase this worktree onto origin/main
    git work check             the gate, from wherever you are
    git work ship              check, push, open the pull request, auto-merge on green
    git work land              after the merge: close the item, remove the worktree
    git work list              worktrees, their branches, their items, their pull requests
    git work clean             reap everything whose branch is gone from the remote
    git work release <VER>     the release branch, prepared and shipped
    git work tag <VER>         tag main once the release has landed

The link between a branch and its backlog item is `branch.<name>.cairn-item` in
git config, which is what per-branch config is for. `ship` reads it to title the
pull request; `land` reads it to close the item. Nothing has to be remembered
and nothing has to be passed twice.

Worktrees live outside the repository, in a sibling directory, so nothing nests
inside a checkout and no ignore rule has to be maintained to hide them.

`make setup` configures the rest of it: the alias, `core.hooksPath`, rerere,
zdiff3 conflicts, autosetupremote, prune on fetch, and the histogram diff.

## Acceptance criteria

- [x] `git work start` produces an isolated worktree with the item claimed
- [x] `git work ship` opens a pull request that merges itself when CI is green
- [x] `git work land` closes the item and leaves no worktree behind
- [x] `make setup` is the only thing a new checkout needs
- [x] The whole loop is documented in HACKING

## 2026-09-07

Verified by using it: this branch was shipped with `git work ship`, which
opened the pull request, filled its body from this item, and turned auto-merge
on. `start`, `land`, `drop`, `list` and `clean` were exercised against item
0064 and the item put back afterwards.

Two things that only showed up by using it, and both are in:

**`land` refused nothing.** A branch with no commits and no pull request is an
ancestor of the trunk, so the reaper happily closed a backlog item because
nothing had been done to it. That is how a backlog quietly loses work. `land`
now refuses that case and `clean` skips it.

**Abandoning is not landing.** `git work drop` hands the item back to the
backlog rather than closing it, which is what changing your mind actually
means.
