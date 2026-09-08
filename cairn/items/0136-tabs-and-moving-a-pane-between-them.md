---
id: 136
title: Tabs, and moving a pane between them
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: mux
effort: l
---

## Problem
A pane is created in a workspace and stays there. Splitting is the only way to
organise, so a workspace with logs, a server, a test watcher and an agent in it
is four panes competing for one screen.

`0041` already tracks the tab layer. This is the part that makes it worth
having.

## Proposal
Alongside tabs: `pane move <pane> --tab <id>`, `--new-tab` and
`--new-workspace`. A pane keeps its process, its scrollback and its agent
identity across the move; only its address changes.

The answer carries the new id and the previous one, because a caller holding the
old id needs to be told rather than left to discover it on its next call. A wait
outstanding against the old address ends with an error naming the move.

Depends on `0041`.

## Acceptance criteria
- [x] `pane move` to an existing tab, a new tab, or a new workspace
- [x] Process, scrollback, agent identity and state survive
- [x] The answer carries both the new and the previous id
- [x] An outstanding wait survives, which is better — see below
- [x] `--current` from inside the moved pane keeps working

## 2026-09-08

The wait criterion turned out to be herdr's problem and not dirk's. herdr's pane
ids are scoped to a workspace, so moving a pane renames it and anything holding
the old name has to be told. dirk's are session-wide counters: `p12` is `p12`
wherever it is, so a wait keeps working and `--current` inside the moved pane
still resolves. Only the qualified `w7:p12` changes, and the answer says what it
was.

`reap` and the move are the same removal — one drops the pane and the other
keeps it — so `take_pane` is the primitive and `reap` calls it. Two copies of
"remove a pane, close the tab if that emptied it, close the workspace if that
was its last tab" is two places for that to stop agreeing.

Boards are not movable. A board's panels are its shape, and taking one out would
leave an arrangement nobody described.

And a real one: `wait::options` only gave a value to the flags the waits used,
so `--tab w1:t1` parsed as a switch and a stray positional. That parser is the
whole surface's now, and its list says so.
