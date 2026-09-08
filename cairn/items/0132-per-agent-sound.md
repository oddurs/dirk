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
- [ ] `[sound.agents]` keyed by harness, values on/off/default
- [ ] Resolution order documented: agent, then project, then global
- [ ] An unknown harness name warns at load rather than silently doing nothing
