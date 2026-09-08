---
id: 154
title: Pointing at it
type: milestone
status: backlog
key: v0.11
created: 2026-09-08
updated: 2026-09-08
due: 2027-10-15
---

The sidebar is the product, and a sidebar is a thing you point at. dirk's is
drawn for the eye and operated by the keyboard: rows teach keys, folding is a
key, the palette is a key, and clicking a row you are already on means the one
thing left for that click to mean.

The mouse is not missing. It is second-class -- a translation of a keyboard
model rather than a way of working in its own right. This milestone inverts
that: everything reachable by pointing, and the interface saying what is
pointable before you point at it.

**Mouse-first, not mouse-only.** This is a terminal. ssh, a machine with no
pointer, a terminal that will not report motion, and thirty years of muscle
memory are all real, and the keyboard stays complete. The principle is written
down and tested rather than believed, because a shift like this rots the other
input the moment nobody is checking.

## Where it goes

Filed after v0.10 because that is where the numbers ran out, which is not an
argument. It is worth moving earlier: v0.8 builds more chrome on the assumption
that the keyboard drives, and two of its items -- `pane borders, and turning the
mouse off` and `resizing a split from the keyboard` -- are the other halves of
work in here. Doing this after them means drawing that chrome twice.
