---
id: 129
title: Reading a full-screen agent's history
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: agents
effort: l
---

## Problem
`pane read` sees the grid vt100 keeps. Claude and opencode draw their transcript
in the alternate screen, so what dirk can read is one screenful and the history
is inside the program, reachable only by scrolling it.

So `agent read --lines 200` quietly returns eighty, and a caller collecting an
agent's answer gets the last screen of it with no indication that the rest
existed.

## Proposal
When the target is a recognised agent, sitting idle at the bottom of its
transcript, and the read asks for more lines than the screen holds: send the
agent the mouse-wheel input it already understands, collect overlapping pages,
stitch them on the overlap, and put its viewport back at the bottom before
answering.

Narrowly, and only there. Not while it is working — an agent redrawing under you
gives you a stitched read of two different screens. Not for a pane somebody is
scrolled back in. Not for a program that never asked for mouse events. Every one
of those returns what it can and says so, rather than moving somebody's viewport
as a side effect of a read.

## Acceptance criteria
- [x] Only for an idle recognised agent, at the bottom, when `--lines` exceeds the screen
- [x] Pages overlap and are stitched on the overlap
- [x] The viewport is returned to the bottom before the answer
- [x] A working, blocked or scrolled agent is refused with a distinct error
- [x] A program that does not report mouse input falls back to the plain read
- [x] No other read path moves a viewport

## 2026-09-08

Built on the held-question machinery from `0117`, and it is the only held
question that *acts*. It has to be: a read that drives another program and waits
for it cannot be answered on the turn it was asked, because that turn is also
the one that would have to process the redraw.

Two things this shook out.

The wheel step is three events, not a screenful. How far a program moves for one
wheel event is its own business — three lines is common and nothing guarantees
it — so asking for a screenful at a time gave pages that did not overlap
whenever the guess was high, and a transcript with holes in it. A short step
overlaps under every guess, and the overlap is the whole safety of the join.

And a real bug, found because the stand-in is a Python program: `python3` on a
Homebrew mac is a shim that execs `.../Python.framework/.../MacOS/Python`, so
the name in the process table is capitalised and nothing matched it. Every agent
written in Python was invisible there — aider included, which dirk ships rules
for.

The viewport is put back before the answer in every exit from this, including a
timeout, whose message says so. A read that leaves somebody's agent scrolled
into its own past is worse than a read that failed.
