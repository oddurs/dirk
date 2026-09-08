---
id: 140
title: Pane borders, and turning the mouse off
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: chrome
effort: s
---

## Problem
Two small ones. Borders are drawn for splits and never for a lone pane, so a
single-pane workspace has no frame and no title strip. And mouse capture is
unconditional, so somebody who wants their terminal's own selection has to stop
using dirk to get it.

## Proposal
`[ui] pane_borders = "auto" | "always" | "off"`, `auto` keeping today's
behaviour. `always` frames a lone pane, which requires drawing outer edges.

`[ui] mouse = false` stops dirk requesting mouse reporting entirely: no clicking
the nav, no drag-select, and the outer terminal's own selection back. All or
nothing, because a half-captured mouse is a mode you have to remember.

## Acceptance criteria
- [ ] `pane_borders` with three values; `auto` is unchanged behaviour
- [ ] `always` frames a single pane
- [ ] `mouse = false` requests no mouse reporting at all
- [ ] With the mouse off, everything reachable by click is reachable by key
- [ ] Both apply on reload without a restart

**Related.** `splits resize by dragging the border between them` needs a
border to grab, so if that lands first this one is its blocker rather than a
separate tidy-up. The `mouse = false` half is the same argument the
mouse-first milestone makes in `when there is no pointer`: all or nothing.
