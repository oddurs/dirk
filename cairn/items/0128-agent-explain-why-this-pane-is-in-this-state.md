---
id: 128
title: 'agent explain: why this pane is in this state'
type: feature
status: done
milestone: v0.7
created: 2026-09-07
updated: 2026-09-08
priority: p1
area: agents
effort: m
---

## Problem
Four ranked signals decide a state and `agent list` names the winner. When the
answer is wrong that is not enough: you need to know what the other three said,
what the screen looked like, and which marker matched or failed to.

Debugging it today means adding a println and rebuilding.

## Proposal
`dirk agent explain <target>` prints the decision: the harness, the final state,
each signal and what it claimed, the marker that matched, the screen region it
matched in, and — when nothing matched — the fallback that was taken and why.

`--file screen.txt --agent claude` runs the same evaluation over a captured
screen with no session at all, which is how a bad detection becomes a test case
instead of a bug report with a screenshot in it.

## Acceptance criteria
- [x] Every signal, its claim, and which one won
- [x] The matched marker and where on the screen it matched
- [x] The fallback and its reason when nothing matched
- [x] `--file` with `--agent` evaluates offline, without a running session
- [x] `--json`, and a test that feeds captured screens through it

## 2026-09-08

One code path, not two. `observe` now calls `explain` and takes the verdict off
it, so what this reports and what the nav believes cannot be two rules that
happen to agree today. The screen rule likewise returns a `Found` — marker,
line, menu required, menu present — and `blocked` is a method on it rather than
a separate function that could drift.

Every answer is already JSON, so `--json` needed nothing; the criterion is met
by the surface dirk already had.

`--file` takes the whole file as the prompt window rather than re-deriving one
around a cursor, because a captured screen has already been cut to the
interesting part by whoever captured it.
