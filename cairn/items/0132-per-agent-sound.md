---
id: 132
title: Per-agent sound
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: agents
effort: s
---

## Problem
Sound is on or off, and a project can ask to be quiet. There is no way to say
that one harness is chatty and the others are not, which is the actual shape of
the problem when you run three at once.

## Proposal
`[sound.agents]` keyed by harness name, each `on`, `off` or `default`, resolved
after the project rule. One more key on a table that already exists.

## Acceptance criteria
- [x] `[sound.agents]` keyed by harness, values on/off/default
- [x] Resolution order documented: agent, then project, then global
- [x] An unknown harness name warns at load rather than silently doing nothing

## 2026-09-08

`default` and absent are the same answer, and both mean "say nothing" rather
than "yes" — so the table only ever overrides, and adding a name you meant to
type later changes nothing until you type it.

A value dirk does not understand also says nothing, and is complained about. A
typo that silently meant `off` is a notification you never hear and never find
out you are not hearing.

The test asserts both directions in two sessions rather than one. Reloading
mid-test does not work: the noise is made on the client, from the client's own
configuration, and that is cached — which is the same split `0033` established
and worth not forgetting.
