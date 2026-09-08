---
id: 185
title: Three overlays, three copies of one renderer
type: chore
status: backlog
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

- [ ] One overlay frame, three callers
- [ ] The box arithmetic and the hit map have unit tests
- [ ] All three look the way they look now
