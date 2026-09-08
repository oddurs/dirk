---
description: Run the gate, push, open the pull request, and let it merge itself
allowed-tools: Bash(git work:*), Bash(scripts/work:*), Bash(make:*), Bash(cargo:*), Bash(git:*), Bash(gh pr:*), Bash(cairn:*), Read, Grep, Glob
---

Ship what is in this worktree.

Before running anything, check the work is actually finished:

1. Every acceptance criterion on the item is met. Tick them off in the item.
2. Behaviour a user can see has a test in `tests/` that would fail without the
   change.
3. If the chrome changed, `make shot` shows what you meant.
4. Anything learned along the way is recorded on the item, not only here.

Then commit, if there is anything uncommitted. Subjects are lowercase and
area-prefixed and say what changed rather than what was done — `boards: a row
that reports without being opened`. The body is prose saying why, with a
GNU-style file list at the end when more than one place changed. Never credit a
tool; the hook strips it anyway.

Then `git work ship`. It runs `make check`, pushes, opens the pull request
titled from the item and bodied from its reasoning, and turns on auto-merge.

If `make check` fails, fix it. Do not ship around it and do not use
`--no-verify`.

Report the pull request URL and what is now waiting on CI.
