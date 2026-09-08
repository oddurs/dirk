---
id: 155
title: Mouse-first, keyboard-complete
type: chore
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: s
area: docs
depends_on:
- 156
- 157
---

## Problem
The mouse works and is second-class. Every gesture in the interface today is a
translation of a keyboard model: clicking a row you are already on folds it,
because that is "the only thing left for that click to mean"; the footers under
each section teach keys and offer no buttons; folding, the palette, finding and
naming are keys with a click bolted beside them where somebody happened to add
one.

Inverting that is the milestone. What the milestone cannot do by itself is stop
the keyboard rotting on the way, and a shift like this rots it quietly: the
next action gets a click and no key, then the one after that, and a year later
dirk does not work over a link that will not report a pointer.

## Proposal
One sentence, in `AGENTS.md` beside the other things that have to be in your
head, and a test that holds it rather than a promise that somebody will:

> **Mouse-first, keyboard-complete.** Every action is reachable by pointing and
> every action is reachable by key. Neither is a translation of the other.

`action.rs` already makes half of this checkable: `Action::ALL` is the table,
and an action that is not in it does not exist. The other half needs the hit
map to be able to say which actions it can reach, which is what makes this
depend on the two items under it rather than being a change to a document.

The interesting case is the gestures that are not actions -- drag to resize,
drag to move, drag to select. Those need a keyboard equivalent named in the
same table, not a footnote admitting the mouse can do more.

## Acceptance criteria
- [ ] The principle is in `AGENTS.md`, in one sentence, with the reasoning
- [ ] A test fails on any action with no default key
- [ ] A test fails on any action nothing in the interface can be pointed at to reach
- [ ] Every pointer gesture that is not an action names its keyboard equivalent
- [ ] Adding an action with only one of the two fails the suite
