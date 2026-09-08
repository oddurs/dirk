---
id: 2
title: PTY panes on vt100
type: feature
status: done
milestone: v0.1
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: mux
---

## Problem
A multiplexer needs a terminal emulator, and writing one is the single largest
piece of work in the project.

## Proposal
Do not write one. `vt100` keeps the grid; `portable-pty` spawns the child and
owns the fd. dirk owns the plumbing either side: a reader thread per pane
feeding the parser, and a blit from that grid into ratatui's buffer.

## Acceptance criteria
- [x] A pane runs a real shell and echoes
- [x] Colours, bold, inverse and wide characters survive the blit
- [x] Resize propagates to the kernel and to the parser
- [x] Mouse events are forwarded when the program inside asked for them
