---
description: Close the item, remove the worktree, and tidy up after a merge
argument-hint: "[branch]"
allowed-tools: Bash(git work:*), Bash(scripts/work:*), Bash(gh pr:*), Bash(git:*), Bash(cairn:*)
---

Land $ARGUMENTS.

`git work land $ARGUMENTS` — with no argument it lands the branch of the
worktree you are in. It checks the pull request actually merged, closes the
backlog item, removes the worktree and deletes the branch.

If it reports the pull request has not merged, do not force it. Say what state
it is in: failing checks, a requested change, or still running.

If the work is being abandoned rather than finished, that is `git work drop` —
it hands the item back to the backlog instead of closing it. Never use `land`
to tidy away work that was not done.

Afterwards, `git work list` to say what else is still in flight, and
`git work clean` if anything else has merged in the meantime.
