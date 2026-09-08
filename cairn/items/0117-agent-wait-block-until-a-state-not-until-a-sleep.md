---
id: 117
title: 'agent wait: block until a state, not until a sleep'
type: feature
status: done
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
- [x] `--until` accepts each state, repeats, and defaults to blocked/done/idle
- [x] Returns immediately when the agent is already in a named state
- [x] The agent exiting ends the wait with a distinct error
- [x] `--timeout` in milliseconds; no timeout means wait indefinitely
- [x] A wait survives the pane being renamed or the workspace being refocused

## 2026-09-07

The whole of it is that the socket thread is *already* blocked on the reply
channel while the event loop answers. So a held question needed no change to
the wire and no second thread: it is one whose reply is sent from a later turn
of the loop. `src/wait.rs` is the shape of a held question; `App::hold`
recognises one and `App::settle` answers the ones that can be.

Settling happens after `update_states`, so a wait for `blocked` ends on the
turn the agent became blocked rather than on the tick after it. And `turn` now
wakes on the nearest deadline as well as on events — with a one-second ticker
as the only other wakeup, a `--timeout 250` was being answered a second late,
which is not a timeout.

Waits are keyed by pane, not by workspace. Renaming and refocusing both happen
to a workspace an agent is working in, continuously, and neither is the thing
being waited for.

A caller that dies while waiting leaves its `Held` in the list until the agent
or pane goes away, because nothing tells the session the far end has hung up.
It is one small struct, and the alternative is probing a channel nobody is
reading.
