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
- [ ] Every command, its arguments, and the shape of its answer
- [ ] Generated from the command table, not written twice
- [ ] Stable ordering, so the diff is readable
- [ ] A test that fails when the checked-in copy is stale
