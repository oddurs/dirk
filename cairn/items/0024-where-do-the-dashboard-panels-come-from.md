---
id: 24
title: Where do the dashboard panels come from?
type: spike
status: backlog
milestone: v0.2
depends_on:
- 35
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: layout
effort: m
---

## Question
The dashboard needs a brief, an attention list, a spaces tree and a cairn board.
smali already draws all four. Does dirk shell out to smali, absorb the panels,
or define a way for any program to be a panel?

## Why it needs answering before the work
It decides whether smali stays alive as a separate project, whether dirk grows a
cairn dependency, and whether the dashboard layout is configuration or code. All
three answers are hard to reverse once the dashboard ships.

## What would settle it
- What smali's panels need that a plain program in a pane does not get. If the
  answer is nothing, shelling out wins on cost.
- Whether the panels need to read dirk's live state. The spaces tree and the
  attention list plainly do, and today they read herdr's socket. That points at
  the socket API (v0.4) being a prerequisite for two of the five panes.
- Whether a panel that is a subprocess redraws fast enough to sit in a layout
  that is always open.

## Answer

<!-- Filled in when the spike closes. -->
