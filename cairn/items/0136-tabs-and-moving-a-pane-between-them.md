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
- [ ] `pane move` to an existing tab, a new tab, or a new workspace
- [ ] Process, scrollback, agent identity and state survive
- [ ] The answer carries both the new and the previous id
- [ ] An outstanding wait against the old id ends with a distinct error
- [ ] `--current` from inside the moved pane keeps working
