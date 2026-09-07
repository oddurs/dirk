---
id: 54
title: 'Tokens: what a name is made of'
type: feature
status: done
milestone: v0.3.1
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: naming
effort: l
---

## Problem
A workspace label is the intent and nothing else. namesync publishes eleven
tokens — project, branch, worktree, n, intent, since, age, locked, stale, agent,
agents — and the label is a template over them. dirk computes three of those and
has no way to arrange them.

The tokens are also an interface. namesync wrote them down in TOKENS.md because
herdr's metadata is a flat untyped map where a producer renaming a key breaks a
consumer with no error and nothing in a build log. dirk owns both ends, so the
contract is cheaper to keep — but the semantics still have to be decided rather
than fall out of whatever the code happens to do.

## The two clocks
`since` restarts whenever the agent changes state; `age` restarts only when the
title changes. An agent that has started and finished six times is still working
on the same task, and `age` is the number that says so. Getting these confused
makes both useless.

## Acceptance criteria
- [x] All eleven tokens computed, and absent rather than empty when unknown
- [x] `since` and `age` are distinct clocks, documented as such
- [x] `n` is display, not identity
- [x] Buckets, not durations — a token is for reading, not arithmetic
- [x] A token nobody sets does not become an empty string in a template
