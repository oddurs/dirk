---
id: 156
title: A pointer is a gesture, not a chain of special cases
type: chore
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: l
area: chrome
---

## Problem
`App::on_mouse` is an ordered chain of special cases: the divider first,
because a drag on it must own everything until the button comes up; then the
wheel over the nav; then the hit map; then the wheel over a pane; then
selection; then whatever is left goes to the program inside. Each clause was
right when it was added and the shape is now the obstacle.

Three things it cannot express.

**A click.** Activation happens on `Down`. There is no press-and-release pair,
so there is no pressed appearance, no cancelling by dragging off the thing you
pressed, and no telling a click from the beginning of a drag. Every button in
every other interface anybody uses has all three.

**A second click.** Nothing remembers when the last one was or what it was on,
so double-click cannot exist -- and neither can the word and line selection
everybody expects from a terminal.

**A grab.** The divider has one, spelled `self.dragging`, and it is the only
one. Every other drag -- a border, a scrollbar thumb, a pane being moved -- has
to invent it again, and each copy is another chance to miss the button-up and
leave the interface stuck in a drag nobody is performing.

## Proposal
A `Pointer` between the raw events and the interface, turning presses into
gestures: `Press`, `Click`, `DoubleClick`, `TripleClick`, `DragStart`, `Drag`,
`Drop`, `Hover`, `Wheel`. It owns the grab -- whoever takes a press keeps every
event until release, wherever the pointer has got to by then -- and it owns the
double-click clock.

`on_mouse` then reads as a match on gestures, and the clauses that survive are
the ones really about where the pointer is rather than about what part of a
sequence it is in.

Not a widget tree. dirk draws in one pass and registers hits as it draws, and
that is load-bearing: there is no second layout pass that can disagree with the
first. This sits between the terminal and that, and does not disturb it.

## Acceptance criteria
- [ ] Gestures come from raw events through one type, tested on its own
- [ ] A press released somewhere else is not a click
- [ ] A grab receives every event until release, including outside its rectangle
- [ ] A grab that never sees its release is ended by the next press, not stuck
- [ ] Double and triple click, with a stated default window and a way to change it
- [ ] `on_mouse` matches on gestures; nothing reconstructs a sequence itself
