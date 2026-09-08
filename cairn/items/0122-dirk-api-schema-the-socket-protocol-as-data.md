---
id: 122
title: 'dirk api schema: the socket protocol, as data'
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: api
effort: s
---

## Problem
The command table is already the single source for `--skill` and the manual. It
is not readable by anything that is not dirk, so a program that wants to speak
the socket protocol has to be taught it by hand, and a test that wants to check
the surface has not silently changed cannot.

## Proposal
`dirk api schema --json` prints the whole request and answer surface, generated
from the same table `--skill` is generated from. Checking the output into the
repository turns "the API changed" from something you notice in a bug report
into a diff in a pull request.

## Acceptance criteria
- [x] Every command, its arguments, and the shape of its answer
- [x] Generated from the command table, not written twice
- [x] Stable ordering, so the diff is readable
- [x] A test that fails when the checked-in copy is stale

## 2026-09-07

The table had to grow a third column first. `(&str, &str)` said what a command
takes and nothing about what comes back, and the answer shape is the half a
caller cannot guess — so `COMMANDS` is now a `Command` with `name`, `args` and
`answer`. Three things read it: the agent skill, `dirk api schema`, and (next)
the completions. None of them is a second copy.

`answer` names keys rather than types. What a caller needs is which key to read;
a type would be a promise about a value dirk does not police.

Answered without a session, like `agent hooks`, because a caller generating a
client wants it before there is one to ask. Also answered *by* a session, so one
already holding a socket does not shell out to ask what it may say down it.
