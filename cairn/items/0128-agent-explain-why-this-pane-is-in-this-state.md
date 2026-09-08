---
id: 128
title: 'agent explain: why this pane is in this state'
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
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
- [ ] Every signal, its claim, and which one won
- [ ] The matched marker and where on the screen it matched
- [ ] The fallback and its reason when nothing matched
- [ ] `--file` with `--agent` evaluates offline, without a running session
- [ ] `--json`, and a test that feeds captured screens through it
