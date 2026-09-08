---
id: 161
title: 'Right-click: every action, where you are pointing'
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: l
area: chrome
depends_on:
- 156
- 157
---

## Problem
There is one way to find out what dirk can do to the thing in front of you: the
palette, which is a key, which lists every action in the program and asks you
to find the four that apply here. It is a good palette. It is not an answer to
"what can I do with *this* space", and there is no answer to that at all for
somebody holding a pointer.

## Proposal
Right-click opens a menu of the actions that apply to what is under the
pointer, anchored where you pointed.

Most of this exists. `Action::ALL` is the table of everything dirk can be asked
to do. `Action::why_not` already answers, per action, why it cannot be done
right now -- and the palette already renders exactly that: entries with their
keys, disabled ones shown with the reason rather than hidden, on the argument
that "a palette that silently omits what it cannot do teaches you that dirk
cannot do it". That argument is stronger here, not weaker.

What is missing is the filter: which actions apply to a *space*, a *checkout*,
a *tab*, a *pane*, a *board*. That is what makes this depend on the hit map
knowing what things are rather than only what clicking them does.

Each entry shows its key. A menu that teaches the keyboard on the way past is
how the two halves of the principle stay honest with each other, and it costs a
column.

Right-click inside a pane is the open question in the ownership spike, and this
item does not decide it.

## Acceptance criteria
- [ ] Right-click opens a menu for the object under the pointer, anchored there
- [ ] Entries come from `Action`, and an action absent from that table cannot appear
- [ ] Inapplicable actions are shown disabled with the reason, not hidden
- [ ] Every entry shows the key that does the same thing
- [ ] The menu is reachable from the keyboard too, on the selected row
- [ ] It closes on Escape, on a click outside, and on choosing something
- [ ] Opening near an edge moves it onto the screen rather than off it
