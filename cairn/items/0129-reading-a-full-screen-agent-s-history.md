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
- [ ] Only for an idle recognised agent, at the bottom, when `--lines` exceeds the screen
- [ ] Pages overlap and are stitched on the overlap
- [ ] The viewport is returned to the bottom before the answer
- [ ] A working, blocked or scrolled agent is refused with a distinct error
- [ ] A program that does not report mouse input falls back to the plain read
- [ ] No other read path moves a viewport
