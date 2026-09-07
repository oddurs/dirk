---
description: Start work on a backlog item — claim it, branch, and open a worktree
argument-hint: <item id or branch name>
allowed-tools: Bash(git work:*), Bash(scripts/work:*), Bash(cairn:*), Read, Grep, Glob
---

Start work on `$ARGUMENTS`.

1. If it is a number, read the item first: `cairn show $ARGUMENTS`. Understand
   the Problem and the Acceptance criteria before touching anything. If the
   body does not tell you enough to start, say so — that is a bug in the item.
   If it is not a number, treat it as a branch name.
2. `git work start $ARGUMENTS` — this claims the item, branches from
   `origin/main`, and creates a worktree beside the repository.
3. `cd` into the path it printed. Everything after this happens there.
4. Read the code the item touches before proposing a change.

Report: the item, what it actually asks for, the worktree path, and your plan.
Do not start editing until the plan is agreed.
