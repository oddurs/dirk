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
The mouse is second-class and the numbers say how much. dirk has twenty-six
actions. **Fifteen of them cannot be reached by pointing at anything.** Every
operation on a pane -- split, close, zoom, move, restart -- is keyboard-only,
and so is every operation on a tab. The character `✕` appears in exactly one
place in the whole interface.

So the interface is not a mouse interface with gaps. It is a keyboard interface
with six buttons on it.

## Proposal
One sentence, in `AGENTS.md` beside the other things that have to be in your
head, and a test that holds it rather than a promise that somebody will:

> **The pointer is how dirk is meant to be used. The keyboard can do
> everything the pointer can.**

The two halves are not symmetric and saying so is the point. The pointer is
what the interface is designed around, what it teaches, and what it shows you
before you know anything. The keyboard is complete -- because this is a
terminal, and ssh from a phone, a link that will not report motion, and thirty
years of muscle memory are all real -- but completeness is a floor, not the
design.

The asymmetry is what stops the obvious failure in each direction. Design for
the keyboard and the mouse is a translation, which is what dirk is now. Design
for the mouse and forget the floor, and dirk stops working in the place it
lives.

## Acceptance criteria
- [ ] The principle is in `AGENTS.md`, in one sentence, with the reasoning
- [ ] A test fails on any action with no default key
- [ ] A test fails on any action nothing in the interface can be pointed at to reach
- [ ] Every pointer gesture that is not an action names its keyboard equivalent
- [ ] Adding an action with only one of the two fails the suite
