---
id: 22
title: Static layouts, declared in configuration
type: feature
status: planned
milestone: v0.2
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
