---
id: 166
title: Double-click and middle-click mean one thing each
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: s
area: chrome
depends_on:
- 156
---

## Problem
A second click means nothing anywhere in dirk, and neither does the middle
button. Both are gestures people arrive already holding: double-click selects a
word in every terminal anybody has used, triple-click takes the line, and
middle-click pastes on every X11 desktop.

The risk is not that they are missing. It is that they get added one widget at
a time, and double-click means "open" on a space, "rename" on a tab and nothing
on a board, because three people each did the obvious thing.

## Proposal
One table, decided once, in the same place the actions are.

| Where | Double | Middle |
| --- | --- | --- |
| In a pane | select the word | paste the selection |
| In a pane, three times | select the line | -- |
| A space row | open it, and zoom its pane | -- |
| A tab | rename it | close it |
| A split border | equalise the two sides | -- |
| The divider | reset the sidebar to its default width | -- |
| A board | open it | -- |

The shape of the table is the point: middle-click closes things and pastes
things, double-click opens things and selects things, and neither ever destroys
anything that is not restorable. Middle-click closing a tab is the one edge --
it is the browser convention, it is what people expect, and it is why a tab
with unsaved work is a question rather than a closure.

Word and line selection need the terminal's idea of a word boundary, which is
also what `copy mode word and paragraph motions` needs from the keyboard side;
they should share it rather than each having an opinion about punctuation.

## Acceptance criteria
- [ ] The table is written down where the actions are, not spread across widgets
- [ ] Double-click selects a word and triple-click a line, sharing one boundary rule
- [ ] Middle-click pastes the selection in a pane
- [ ] Middle-click on a tab closes it, and asks first when it holds live work
- [ ] Nothing in the table destroys anything unrecoverable
- [ ] A widget with no entry does nothing, rather than inventing one
