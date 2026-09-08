---
id: 173
title: An overlay closes the way an overlay closes
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: chrome
depends_on:
- 156
- 157
---

## Problem
dirk has five overlays -- the project picker, the palette, search results, the
prompt that asks a question, and soon a context menu -- and none of them
behaves the way a pointer expects an overlay to behave.

They are dismissed with Escape. Clicking outside one does not close it. Clicking
outside one may well do something else, because the guards are `self.picker
.is_none()` scattered through `on_mouse` rather than a rule about layers, and
each new overlay is another place somebody has to remember to add a clause.
None of them has a way out you can point at.

## Proposal
Three things, and the third is the one that stops this recurring.

**Click away closes it.** The universal gesture. It closes and does nothing
else -- the click that dismissed is spent on dismissing, which is what people
expect and is also the safe reading when the thing behind is a pane.

**A way out you can point at.** An `✕`, in the same place on each of them.

**Layers in the hit map rather than guards in the handler.** An overlay
registers its spots on a layer above; a press outside the topmost layer is a
dismissal, not a hit on what is underneath. `HitMap::at` already takes the last
match, so this is a rule about where a layer starts rather than a new mechanism
-- and it means the next overlay does not have to be remembered anywhere.

The prompt that asks a question is the exception: a question with a default is
dismissible, and one without has to be answered. It says which it is by whether
it draws the `✕`.

## Acceptance criteria
- [ ] Clicking outside an overlay closes it and does nothing else
- [ ] Each overlay has a pointable way out, in the same place
- [ ] Layering lives in the hit map; no handler has an overlay-is-open clause
- [ ] A press cannot reach anything under an open overlay
- [ ] A question with no default is not dismissible, and shows no way out
- [ ] Adding an overlay needs no change to the pointer handler
