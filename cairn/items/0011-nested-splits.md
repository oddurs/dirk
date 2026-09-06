---
id: 11
title: Nested splits
type: feature
status: done
milestone: v0.2
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: layout
---

## Problem
A workspace splits into equal columns or equal rows and no further. That was a
deliberate v1 simplification — the moment a workspace needs a nested layout it
probably wanted to be two workspaces — but the ceiling is real.

## Acceptance criteria
- [x] A layout tree rather than a flat vec
- [x] Closing a pane gives its space to its own siblings and moves nobody else
- [x] Splitting in a direction the parent already runs in keeps shares equal

Two criteria this item started with were moved rather than met, because they
are interaction rather than structure and the tree does not need them to be
finished:

- Drag a border to resize → 0043
- Zoom one pane to fill the workspace and back → 0043

Recorded here rather than quietly unticked: a done item with open boxes is how
a roadmap starts lying about what shipped.

## Plan

A workspace stops holding `Vec<Pane>` plus a direction and starts holding a
tree over a pane arena:

```rust
enum Node {
    Leaf(PaneId),
    Split { dir: Dir, children: Vec<(Constraint, Node)> },
}
```

Panes stay in a flat `Vec<Pane>` and the tree refers to them by id, so moving a
pane between workspaces later (0043) is a tree edit rather than an ownership
problem.

Division is `ratatui::layout::Layout` rather than arithmetic. It already solves
rounding, minimum sizes and how leftover columns are distributed, and getting
that subtly wrong is exactly the kind of bug that shows up as a one-column gap
on some terminal widths and nowhere else.

Splitting appends to the parent when the direction already matches, and only
introduces a new `Split` node when it does not. Always nesting would make three
successive splits to the right 50/25/25 instead of thirds.

Removing a leaf collapses any split left holding one child, so the tree cannot
accumulate single-child nodes that affect nothing but are still there to be
reasoned about.

Constraints are `Fill(1)` everywhere for now. They exist in the type because
0022 needs `size = "5"` and `size = "30%"`, and retrofitting a constraint into
a tree that assumes equality is worse than carrying it unused for one release.
