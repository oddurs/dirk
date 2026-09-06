---
id: 31
title: Real lifecycle states, including seen
type: feature
status: backlog
milestone: v0.3
depends_on:
- 30
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: agents
effort: l
---

## Problem
The state glyph is inferred from whether a pane has ever published a title, so
everything that has looks like it is working. `blocked` — the only state that
is waiting on a human — is never shown, which makes the glyph column decorative.

## Proposal
Four states with the semantics herdr uses, because they are the right ones:

    working   producing output
    blocked   an approval or question is on screen
    done      idle, after work you have not looked at
    idle      idle, and seen

`done` and `idle` are the same underlying state; what separates them is whether
you have seen it. Focusing the space marks it seen. Reading state over the API
does not — otherwise a status line would clear your own notifications.

Detecting blocked means recognising an approval prompt in the pane's grid. Start
with the known agents' own prompts rather than a general heuristic.

## Acceptance criteria
- [ ] Four states, distinguished for real
- [ ] Focus marks seen; API reads do not
- [ ] blocked detected for Claude Code's approval prompt
- [ ] A state change is an event, so the nav redraws without polling
- [ ] Naming holds off while blocked — the ported policy has the branch and
      cannot currently reach it (see 0009)
