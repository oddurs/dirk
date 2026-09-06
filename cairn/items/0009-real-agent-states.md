---
id: 9
title: Real agent states
type: feature
status: dropped
milestone: v0.3
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: agents
---

## Superseded

Split into three items that say what this one gestured at: 0030 detects the
agent, 0031 gives it real lifecycle states including the seen rule, and 0032
makes the rail counts mean something. Dropped rather than closed, because
nothing was built for it.

## Problem
The sidebar's state glyph is currently inferred from whether a pane has ever
published a title: anything that has is 'working'. So the one state that
matters — blocked, the only state waiting on a human — is never shown.

## Proposal
Detect the agent from the process tree and read its state the way herdr does,
rather than guessing from the title.

## Acceptance criteria
- [ ] blocked, working, done and idle are distinguished for real
- [ ] The rail's counts mean something
- [ ] Naming holds off while an agent sits on an approval dialog, which the
      ported policy already has a branch for and cannot currently reach
