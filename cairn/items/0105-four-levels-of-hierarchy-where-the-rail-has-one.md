---
id: 105
title: Four levels of hierarchy where the rail has one
type: feature
status: backlog
milestone: v0.5
labels:
- rail
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: chrome
---

## Problem

Outside the mark and the chip bars, every element in the rail is
`THEME.dim()`: the space count, the clock, the scroll depth, detach. The clock
and the way to end a day's work are the same colour and the same weight.

The rail also paints one flat ground and then never uses it. In a surface one
row tall, background is the strongest signal available, and it is currently
spent on nothing.

Two states suffer most from this. A chord waiting for its second key is the most
time-critical feedback in the interface and it is dim text in the middle of a
sentence. And being scrolled back is a mode — the keys mean different things and
the screen is not live — rendered as ambient text beside the clock, which is
exactly the failure the file already names: *"a pane being read from the past
looks exactly like a program that has stopped."*

## Proposal

Four levels, and the top one is the only place an inverted ground is ever used,
which is what makes it unmissable.

| | What | Treatment |
| --- | --- | --- |
| 1 | Prefix armed · blocked · quit armed | Inverted ground, bold |
| 2 | The user's name · the host · the focused intent · detach | Full-strength text |
| 3 | Project · done · quit | Muted |
| 4 | Clock · separators · pane counter | Faint |

The prefix indicator moves out of `note` and into the context slot, displacing
it: mid-chord, nothing else on the bar matters. Scroll depth moves out of the
dim run and into the attention group, where the other things that are true
right now live.

Colours come from the roles that already exist in `theme.rs`; the inverted
pairings need adding there rather than being patched together at the call site.

## Acceptance criteria

- [ ] Exactly three things can take an inverted ground, and nothing else does
- [ ] The prefix indicator is legible at a glance without reading the bar
- [ ] Scroll depth sits with attention, not with the clock
- [ ] No call site in `rail.rs` names a colour; every one asks for a job
- [ ] Both glyph sets still read correctly, ascii and unicode

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0
