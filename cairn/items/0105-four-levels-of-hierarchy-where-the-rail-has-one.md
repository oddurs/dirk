---
id: 105
title: Four levels of hierarchy where the rail has one
type: feature
status: done
milestone: v0.5
assignee: oddurs
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

- [x] Exactly three things can take an inverted ground, and nothing else does
- [x] The prefix indicator is legible at a glance without reading the bar
- [x] Scroll depth sits with attention, not with the clock
- [x] No call site in `rail.rs` names a colour; every one asks for a job
- [x] Both glyph sets still read correctly, ascii and unicode

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0

## 2026-09-07

Three things take a ground of their own and nothing else does: the prefix
while a chord is open, the blocked count, and quit once it is armed. Two new
roles in `theme.rs` carry them, so no call site in `rail.rs` names a colour.

The prefix moved out of `note`, which is where it used to be smuggled — main
passed the word "prefix" as a status string and the bar drew it in the same dim
as the clock. It is its own field now and displaces where you are, because
mid-chord nothing else on the bar matters.

Scroll depth moved the other way, out of the dim run and in beside what is
owed. It is a mode: the keys mean different things and the screen is not live.

The ascii set was checked rather than assumed — `# oddurs   dirk > 2 Reading
the vt100 grid > 2/2 ... x quit    detach`, and `x` and `^` for the exits once
the words go.

Adding the roles found a bug in the website: its palette generator read only
the line after a method, so a role whose body `cargo fmt` had split across
lines vanished from the stylesheet with nothing noticing — which is the exact
failure that file exists to prevent. It reads whole bodies now.

## 2026-09-07

A code review caught the thing that made all of this do nothing.

`Seg::plain` was `style.patch(THEME.rail())`, and ratatui`s `patch` lets the
*argument* win — so the rail`s own foreground was handed to every role and the
bar rendered as one flat `#d3ebe9`. Eight runs, one colour. The accent on the
mark and the number, the muted project, the faint crumbs and clock, the mauve
session, the peach host and scroll depth, the green done count: all inert. Only
the two inverted roles survived, because they set a ground as well.

The order is `THEME.rail().patch(style)`, and it had been the wrong way round
for as long as the file has existed — the old bar was not "everything in one
dim", it was everything in one *text* colour, and the mark was accent only
because it was the one thing written without patching at all.

Also from the review: a chord that did not fit returned an empty middle, which
made that step succeed and stopped the ladder — so the indicator was dropped
while the clock and the full-word exits were still on the bar. It returns
`None` now, like every other thing in that slot, and costs a step.
