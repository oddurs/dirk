---
id: 22
title: Static layouts, declared in configuration
type: feature
status: done
milestone: v0.2
assignee: oddurs
depends_on:
- 11
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: layout
effort: l
---

## Problem
There is no way to say 'open these five programs in this arrangement and call it
Overview'. A dashboard has to be built by hand every time, which means it is not
built.

## Proposal
A layout is a named split tree with a command at each leaf, declared in
config.toml and opened from the top section of the nav:

    [[layout]]
    name = "Overview"
    split = "rows"

      [[layout.pane]]
      title = "brief"
      command = ["smali", "brief"]
      size = "5"

      [[layout.pane]]
      split = "cols"
        [[layout.pane.pane]]
        title = "ptop"
        command = ["ptop"]
        [[layout.pane.pane]]
        title = "spaces"
        command = ["smali", "tree"]

Sizes are lines or a percentage; a pane with no size shares what is left.

A layout opens as a space of its own rather than as an overlay over the current
one. A heads-up display that is always in peripheral vision is not a heads-up
display, it is furniture you stop seeing.

Blocked by the split tree (0011): equal columns cannot express this.

## Acceptance criteria
- [ ] Nested splits with explicit and proportional sizes
- [ ] Each pane titled on its border
- [ ] A layout opens as its own space, listed in the layouts section
- [ ] Closing the last pane closes the layout
- [ ] A layout whose program is missing opens with that pane showing why,
      rather than failing the whole layout
- [ ] Re-opening a layout that is already open focuses it

## Plan

The insight that makes this small: **a layout is a workspace**. It has panes, a
split tree, a focused pane and a name — which is the whole of `Workspace`. So
`Page { def, pane: Option<Pane> }` becomes `Layout { def, ws: Option<Workspace> }`
and the separate single-pane rendering path for pages disappears. Two code paths
become one, and this feature is mostly deletion.

A page was already a one-pane layout, so `[[pages]]` becomes `[[layout]]` rather
than gaining a sibling.

Building a layout is three passes, because ids and geometry depend on each
other: walk the definition assigning pane ids and building the `Node`; ask the
tree for rects over the content area; then spawn each program at the size it is
actually going to get. Spawning first and resizing after would show every
program one redraw at the wrong size, which for a full-screen TUI is a visible
flash.

Sizes parse to `ratatui::Constraint` — `"5"` to `Length`, `"30%"` to
`Percentage`, absent to `Fill(1)`. A size that does not parse is a config error
reported before the terminal is taken over, where it can still be read.

Deferred: `0015`'s overlay placement and per-pane working directories, which are
about how a layout is *entered*, not what one is.
