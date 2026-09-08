---
id: 117
title: 'agent wait: block until a state, not until a sleep'
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: api
effort: m
---

## Problem
A script that wants to know when an agent has stopped has to poll `agent list`
on a timer. Everyone writes that loop, everyone picks a different interval, and
everyone gets it slightly wrong at the edges.

## Proposal
`dirk agent wait <target> --until blocked` returns when the agent reaches one of
the named states, or immediately if it is already in one. `--until` repeats, and
defaults to the three states that mean the agent has stopped needing the
processor: `blocked`, `done`, `idle`.

dirk already has the state machine and the ranked signals. This is the state
machine given an edge-triggered interface instead of a polled one.

An agent that exits while a wait is outstanding ends the wait with an error
naming that, rather than with a timeout — the two mean different things to the
caller.

## Acceptance criteria
- [ ] `--until` accepts each state, repeats, and defaults to blocked/done/idle
- [ ] Returns immediately when the agent is already in a named state
- [ ] The agent exiting ends the wait with a distinct error
- [ ] `--timeout` in milliseconds; no timeout means wait indefinitely
- [ ] A wait survives the pane being renamed or the workspace being refocused
