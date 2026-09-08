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
- [x] `ui.pane_rules` with three values; `auto` is unchanged behaviour
- [x] `always` rules every pane, a single one included
- [x] `mouse = false` requests no mouse reporting at all
- [x] With the mouse off, everything reachable by click is reachable by key
- [x] Both apply on reload without a restart

## 2026-09-08

Not borders. dirk draws no boxes and says why in `src/main.rs`: three sides of
one only repeat what the neighbouring pane's own edge already says, and the
fourth is a row of terminal nobody gets to use. What it draws is a rule — the
top line, carrying the label and the focus mark — and only for a pane with a
label, which is a board's panel.

The gap underneath the item is real, though: two shells side by side have no
visible boundary at all and nothing says which has the keyboard. So the setting
is `pane_rules`, with the same three values doing the same three jobs.

`content_of` is the one place the arithmetic lives and drawing and resizing must
agree about it, so the policy is cached on the `Session` as well as read where
things are drawn — the same shape `shell` and `terminal` already have.

`mouse = false` drops mouse events as well as declining to ask for them. The
setting means dirk does not use the mouse, and that has to stay true when
something else has turned reporting on — an outer multiplexer, or a terminal
that reports without being asked. It also makes the behaviour testable, which
asking-and-not-asking is not.

Decided on the client, like sound and notifications: that is the end with the
pointing device, and `--remote` should do what the laptop says.

**Related.** `splits resize by dragging the border between them` needs a
border to grab, so if that lands first this one is its blocker rather than a
separate tidy-up. The `mouse = false` half is the same argument the
mouse-first milestone makes in `when there is no pointer`: all or nothing.
