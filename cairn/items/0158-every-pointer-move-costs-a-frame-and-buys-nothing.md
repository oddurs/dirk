---
id: 158
title: Every pointer move costs a frame and buys nothing
type: bug
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: m
area: perf
---

## What happens
`EnableMouseCapture` turns on `?1003h` -- any-event tracking -- so the terminal
reports a `Moved` for every cell the pointer crosses, whether or not a button
is down. Nothing in dirk consumes `Moved`. It falls through `on_mouse` to
`send_mouse`, and `encode_mouse` correctly drops it for any pane that did not
ask for motion.

Correctly, and not cheaply. Each of those events is:

1. a message across the wire from the client to the server,
2. a wake of the event loop,
3. a full `update_states` pass -- which locks *every* agent pane's terminal and
   reads its screen,
4. a full `terminal.draw`, re-rendering the whole interface and rebuilding the
   hit map.

Dragging the pointer across an eighty-column terminal is on the order of eighty
complete re-renders and eighty agent-state passes, none of which change a
single cell. It is invisible, which is why it has lasted: nothing on screen
moves, so nothing looks wrong.

## What should happen
Either spend the events or stop paying for them. Both, really:

- The client coalesces motion. Only the most recent position matters, and
  sending the intermediate ones is describing a path nobody is going to read.
- The loop redraws for a pointer move only when the move changed something --
  which, once hover exists, means when the hovered spot changed. Crossing four
  cells inside one row is one frame, not four.
- `update_states` does not belong on this path at all. It is there because it
  is once per loop iteration, and a pointer move should not be an iteration
  that does agent detection.

This is filed as a bug rather than as perf work because the cost is already
being paid for a feature that does not exist yet. Hover is the item that makes
it worth paying; this is the item that stops it being waste either way.

## Reproduction
1. Attach a client and open two or three spaces with agents in them.
2. Move the pointer steadily across the width of the terminal.
3. Every cell crossed produces a wire message, an agent-state pass and a full
   redraw, and the screen does not change.
