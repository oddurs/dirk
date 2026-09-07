---
id: 31
title: Real lifecycle states, including seen
type: feature
status: done
milestone: v0.3
assignee: oddurs
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
- [x] Four states, distinguished for real
- [x] Focus marks seen; there is no API yet, and when there is, that rule belongs in the protocol
- [x] blocked detected for Claude Code's approval prompt
- [x] A state change is an event, so the nav redraws without polling — states are recomputed on output and on the tick, both of which already draw
- [ ] Naming holds off while blocked — the policy has the branch and now has a
      real `blocked` to reach it with, but the two are not yet wired together.
      Split out as 0052.

## Plan

Four states from three questions, asked in order of what they cost:

1. **Is there an approval prompt on screen?** → blocked. Read from the bottom
   rows of the pane's grid, because that is where a prompt is; scanning the
   whole screen would match an agent that merely wrote the words.
2. **Has it produced output recently?** → working. An agent that is thinking
   redraws its spinner continuously, so silence is the signal, not noise.
3. **Otherwise: have you looked at it since it stopped?** → done if not, idle
   if so.

`done` and `idle` are the same underlying state and only the seen flag separates
them, which is the whole reason `done` exists: finished work nobody has noticed
is the thing worth showing. A workspace is marked seen while it is focused, and
unseen the moment it starts working again. Reads over the API must not mark it
seen — that rule belongs in the protocol, and there is no protocol yet.

Blocked markers are per agent kind, in the same table as the names. They are the
part most likely to be wrong: they are strings another program prints, and it
can change them. So the mechanism is tested separately from the markers, and a
wrong marker degrades to "never blocked" rather than to nonsense.
