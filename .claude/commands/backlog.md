---
description: File, find or update work in the backlog
argument-hint: "[what to file, or an item id]"
allowed-tools: Bash(cairn:*), Read, Grep, Glob
---

Use the `backlog` subagent for $ARGUMENTS.

Search before filing — `cairn search`, then `cairn list --json`. A duplicate is
worse than nothing, and half of what gets asked for is already filed with the
reasoning attached.

An item carries the thinking that produced it: the Problem as it actually is,
the Proposal, and acceptance criteria someone else could check. Never create a
TODO, PLAN or NOTES file instead.

`cairn check` must pass before you report finished.
