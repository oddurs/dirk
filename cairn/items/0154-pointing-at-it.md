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
drawn for the eye and operated by the keyboard.

How much so is measurable. dirk has twenty-six actions. **Fifteen cannot be
reached by pointing at anything.** Every operation on a pane -- split, close,
zoom, move, restart -- is keyboard-only, and so is every operation on a tab
that is not "go to it". The character `✕` appears once in the whole interface.
It is not a mouse interface with gaps; it is a keyboard interface with six
buttons on it.

This milestone makes the pointer the way dirk is used. Not a co-equal path --
the primary one, the one the interface is designed around and teaches. The
keyboard stays complete, because this is a terminal and ssh from a phone is
real, but completeness is a floor rather than the design.

## The shape of it

**The pointer has to work at all.** Gestures instead of a chain of special
cases; a hit map that says what a thing *is* rather than only what clicking it
does; the motion events already arriving for every cell crossed, which cost a
frame each and buy nothing; and a decision about who owns the mouse inside a
pane, which four separate gestures would otherwise each answer differently.

**Then things to point at.** Right-click menus are the backbone -- they reach
twelve of the fifteen on their own. Panes get a strip with their own controls.
Tabs get a strip. The things you close get something to press. Overlays close
the way overlays close. Hover, affordances, scrollbars, draggable borders.

**Then the product questions.** What happens with no pointer. What the footers
full of key hints are for once pointing is primary.

## Where it goes

Filed after v0.10 because that is where the numbers ran out, which is not an
argument. It is worth moving earlier: v0.8 builds more chrome on the assumption
that the keyboard drives, and three of its items -- pane borders, turning the
mouse off, and resizing a split from the keyboard -- are the other halves of
work in here. Doing this after them means drawing that chrome twice.
