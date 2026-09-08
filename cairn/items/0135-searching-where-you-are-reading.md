---
id: 135
title: Searching where you are reading
type: feature
status: done
milestone: v0.8
created: 2026-09-07
updated: 2026-09-08
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
- [x] `/` and `?` inside copy mode search the focused pane only
- [x] `n` and `N` repeat forward and backward
- [x] Smart case
- [x] The cursor lands on the match, where `v` starts a selection
- [x] Escape clears the search before it leaves copy mode
- [x] Session-wide find is untouched

## 2026-09-08

While a query is being typed every key belongs to it, which is the whole reason
this is a state rather than a prefix: a letter that moved the cursor would be a
letter you could not search for — and `y`, `n` and `q` are all letters somebody
searches for.

It wraps. A search that stopped at the end of the screen would be one you had to
know the shape of, and the screen is a window over a scrollback that the cursor
is already free to move through.

An empty query matches nothing rather than everything, so backspacing a query
away leaves you where you are instead of jumping to the top.

The match is where the cursor lands rather than a highlight of its own: `v`
there starts a selection from it, which is the thing you were going to do next
and needs no second mechanism.
