---
id: 185
title: Three overlays, three copies of one renderer
type: chore
status: done
created: 2026-09-08
updated: 2026-09-08
priority: p3
area: chrome
effort: m
---

## Problem

`src/ui/picker.rs`, `src/ui/palette.rs` and `src/ui/found.rs` each render a
centred box with a prompt on its first row, a hint on the second, and a list of
rows from the third. Each is about 110 lines and they are the same 110 lines:
the same `box_area` arithmetic, the same `fill`, the same `y = box_area.y + 2 +
row`, the same per-row `Rect`, the same hit registration, the same highlight
for the selected row.

Between the three of them there are no unit tests.

That combination is the cost: a change to how an overlay looks has to be made
three times, there is nothing to catch the copy that was missed, and the only
way to see the difference is to open all three.

## Proposal

One `overlay` in `src/ui/` that owns the frame, the prompt row, the hint row,
the row rectangles and the hits, and takes the rows and a closure that draws
one. Each caller keeps what is actually its own — the picker's score-ordered
paths, the palette's right-aligned reasons, the found list's match context.

With the arithmetic in one place it can be tested directly, which is the point
rather than the line count.

## Acceptance criteria

- [x] One overlay frame, three callers
- [x] The box arithmetic and the hit map have unit tests
- [x] All three look the way they look now

## 2026-09-08

The last criterion held except in one place, and the exception is the point of
having done it.

Two of the three kept the selection on screen by starting the list at
`selected - (rows - 1)` when it had run past the bottom. The picker did not: it
took the first `rows` matches and stopped, so holding an arrow down walked the
selection off a list that never scrolled — invisible, and unreachable by the
key that was moving it. The window is in the frame now, so it is in all three.

The picker was also drawing its cursor and its rule as literal `▌` and `─`
rather than asking the glyph table, which is the thing `0066` was about. It
takes the glyphs now like the other two, so `[nav] glyphs = "ascii"` reaches it.

Six tests where there were none, and the ones worth having are the edge cases
none of the three had ever been asked about: a screen smaller than the smallest
box the overlay asks for, an empty list, and a row's hit landing where the row
was drawn after the list has scrolled.
