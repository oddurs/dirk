---
id: 24
title: Where do the dashboard panels come from?
type: spike
status: done
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

**Shell out. There is no panel protocol, and smali stays its own project.**

Settled by building the dashboard rather than by reasoning about it. Once a
layout pane ran in the focused workspace's directory (0015) and stopped
vanishing when its program exited (0049), `cairn roadmap` and `cairn board`
worked as panels with no cooperation from either side — a plain program in a
pane, printing and exiting, its output left on screen. That is the whole
protocol, and it already existed.

Answering the three sub-questions as posed:

**What smali's panels need that a plain program does not get.** For the cairn
board and the system monitor: nothing. They read the filesystem and print. The
cost of shelling out is one process per panel, paid once, since a panel that
exits now keeps its output rather than being restarted.

**Whether panels need dirk's live state.** Two of the five do. A spaces tree and
an attention list are about dirk's own session, and nothing outside dirk can see
it — smali reads herdr's socket for exactly this. So those two are blocked on
the socket API (0035, v0.4), and until then the nav already shows both, which is
where they are more useful anyway. The dashboard ships with three panels rather
than five and loses nothing.

**Whether a subprocess panel redraws fast enough.** Not a question for the
panels that print and exit — they redraw never. `ptop` is a full TUI in a pane
and behaves exactly as it does in any other terminal.

The consequence worth writing down: dirk grows no dependency on cairn or smali.
The default dashboard names them and is dropped entirely on a machine that does
not have them, which is what `runnable` already does for every layout.
