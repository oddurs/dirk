---
id: 161
title: 'Right-click: every action, where you are pointing'
type: feature
status: backlog
milestone: v0.11
depends_on:
- 156
- 157
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: xl
area: chrome
---

## Problem
There is one way to find out what dirk can do to the thing in front of you: the
palette, which is a key, which lists every action in the program and asks you to
find the four that apply here. It is a good palette and it is not an answer to
"what can I do with *this*", and there is no answer to that at all for somebody
holding a pointer.

This is also the mechanism that closes most of the gap. Of the fifteen actions
with no pointer route, a context menu reaches **twelve** -- everything about
panes and tabs -- in one piece of work. Nothing else in the milestone comes
close to that, which is why this is the backbone rather than one feature among
several.

## Proposal
Right-click opens a menu of what applies to the thing under the pointer,
anchored where you pointed.

Most of it exists. `Action::ALL` is the table of everything dirk can be asked to
do. `Action::why_not` already answers, per action, why it cannot be done right
now. The palette already renders exactly that -- entries with their keys,
disabled ones shown with the reason rather than hidden, on the argument that "a
palette that silently omits what it cannot do teaches you that dirk cannot do
it". That argument is stronger here, not weaker.

What is missing is which actions belong to which object, and that is the work:

| Pointed at | Offers |
| --- | --- |
| A pane | split either way, zoom, close, restart, read, move back or on |
| A pane's strip | the same, and rename |
| A tab | new, close, rename, next, previous |
| A space | open, new tab, new agent, release the name, close |
| A checkout | new space here, new worktree, open in a new space |
| A project | open a project, fold, new space, new worktree |
| A board | open, restart |
| The nav's ground | new space, open project, hide the nav |
| A selection | copy, find this |
| The rail | detach, quit, hide the nav, the palette |

Each entry shows its key. A menu that teaches the keyboard on the way past is
how the two halves of the principle stay honest with each other, and it costs a
column.

Right-click inside a pane is the open question in the ownership spike, and this
item does not decide it -- but the menu is the reason the answer matters, and
"the program gets every right-click forever" would take the backbone out of
this milestone.

## Acceptance criteria
- [ ] Right-click opens a menu for the object under the pointer, anchored there
- [ ] Every object in the table above has a menu
- [ ] Entries come from `Action`; an action absent from that table cannot appear
- [ ] Inapplicable actions are shown disabled with the reason, not hidden
- [ ] Every entry shows the key that does the same thing
- [ ] Reachable from the keyboard too, on the selected row
- [ ] Closes on Escape, on a click outside, and on choosing something
- [ ] Opening near an edge moves it onto the screen rather than off it
- [ ] Submenus are not used; a menu that needs one is two menus
