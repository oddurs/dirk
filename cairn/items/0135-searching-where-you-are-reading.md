---
id: 135
title: Searching where you are reading
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: mux
effort: m
---

## Problem
`/` searches every pane in the session, which is the right default and better
than what tmux does. It is not what you want when you are already in copy mode
looking at one pane's scrollback: the results leave, and the answer arrives as a
list of places rather than as a cursor two hundred lines up from where you are.

## Proposal
Inside copy mode, `/` and `?` search this pane forward and backward, `n` and `N`
repeat in the same and opposite direction. Case-insensitive unless the pattern
contains an uppercase letter, which is the convention every tool in this space
already uses.

The match takes the selection highlight, so it is ready to copy. Session-wide
find is unchanged and stays on `/` outside copy mode.

## Acceptance criteria
- [ ] `/` and `?` inside copy mode search the focused pane only
- [ ] `n` and `N` repeat forward and backward
- [ ] Smart case
- [ ] The match is highlighted and ready to copy
- [ ] Escape clears the search before it leaves copy mode
- [ ] Session-wide find is untouched
