---
id: 169
title: Fifteen of twenty-six actions cannot be reached by pointing
type: bug
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: s
area: chrome
depends_on:
- 161
- 170
- 171
- 162
---

## What happens
dirk has twenty-six actions in `Action::ALL`. Eleven can be reached by pointing
at something. The other fifteen cannot be reached with a mouse at all:

| | |
| --- | --- |
| Panes | split into columns, split into rows, close, cycle, zoom, move back, move on, restart |
| Tabs | new, close |
| Reading | find |
| The interface | give the nav the keyboard, hide the nav, the palette, release a held name |

Every operation on a pane is keyboard-only. Every operation on a tab that is
not "go to it" is keyboard-only. The character `✕` appears once in the entire
interface, in the rail.

That is not an interface with gaps in its mouse support. It is a keyboard
interface with six buttons on it, and the milestone's premise -- that pointing
is how dirk is meant to be used -- is false until this list is empty.

## What should happen
Every one of the fifteen has a route you can point at. This item is the list
and the check; the routes themselves are built by the items it depends on:

- **The context menu** takes twelve of them -- everything about panes and tabs.
  One piece of work, most of the list.
- **The pane strip** takes zoom, close and split again as direct controls,
  because the common ones should not need a menu.
- **The close buttons** take close-pane and close-tab likewise.
- **Find** and **the palette** need somewhere to press in the rail or the nav's
  footers, which is the affordance pass.
- **Hide the nav** is the divider's own gesture: dragging it shut, and a
  control on it to do that in one press.
- **Release a held name** belongs to the space's menu, and nowhere else --
  it is rare and it is not worth a permanent control.
- **Give the nav the keyboard** is the one that may not need a pointer route at
  all: clicking in the nav *is* the pointer's version of it. If that is the
  conclusion, it gets written down as an exception with a reason rather than
  quietly passing.

## Reproduction
1. `sed -n '/^actions! {/,/^}/p' src/action.rs` -- twenty-six.
2. `sed -n '/pub enum Target {/,/^}/p' src/hit.rs` -- the ones with a route.
3. Try to split a pane, close a tab, or zoom, without touching the keyboard.
