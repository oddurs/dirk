---
id: 165
title: Drag a pane to where you want it
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: l
area: mux
depends_on:
- 156
- 157
- 160
---

## Problem
A pane goes where it was split. Moving it means `{` and `}`, which walk it
through the tree a position at a time, and there is no way at all to move a
pane into another tab or another space. The nav shows the whole arrangement --
project, checkout, space, tab, pane -- as a tree you can see and not touch.

For a pointer that is the obvious gesture and its absence is loud. A tree of
things in a sidebar is a thing people drag.

## Proposal
Drag a pane onto another pane and it moves there. Drag it onto a tab and it
joins that tab; onto a space and it joins that space's current tab; onto the
edge of a pane and it splits against that edge.

Two hard parts, and neither is the dragging.

**Saying where it will land before it lands.** A drop that has to be guessed at
is worse than no drop. The target edge or pane is drawn as it will be, and if
that cannot be shown clearly then the drop is not offered.

**Refusing the drops that make no sense**, visibly. A pane cannot be dropped
into itself, and a drop that would empty a tab has to say what it will do with
the tab rather than doing something surprising to it. A target that will not
accept says so by not lighting up.

Tabs reorder by the same gesture, which is nearly free once drop targets exist
and is the second thing people will try.

Not spaces between projects. A space belongs to the checkout it is open in --
that is where its branch comes from -- and dragging one to another project
would mean either moving a directory or quietly lying about which checkout it
is in.

## Acceptance criteria
- [ ] A pane drags onto another pane, a tab, or a space, and lands there
- [ ] Dropping on an edge splits against that edge
- [ ] The landing place is drawn before the drop, and matches what happens
- [ ] Targets that will not accept do not light up
- [ ] A tab left empty by a move is handled, and the handling is stated
- [ ] Tabs reorder by dragging
- [ ] Escape during a drag abandons it and puts nothing anywhere
