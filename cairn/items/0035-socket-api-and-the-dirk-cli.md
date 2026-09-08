---
id: 35
title: Socket API and the dirk CLI
type: feature
status: done
milestone: v0.4
assignee: oddurs
depends_on:
- 7
created: 2026-09-06
updated: 2026-09-07
priority: p0
area: api
effort: xl
---

## Problem
An agent working inside dirk cannot see or change anything about the session it
is in. This is the feature that makes a multiplexer agentic rather than merely a
place agents happen to run: herdr's own value is largely that `herdr pane split`
works from inside a pane.

## Proposal
Newline-delimited JSON over a unix socket, and a CLI that speaks it. The command
groups follow the nouns that already exist:

    dirk workspace list|create|focus|rename|close
    dirk pane list|split|focus|read|send-keys|close|resize|zoom
    dirk layout list|open
    dirk agent list|start|read|send-keys
    dirk session list|attach

Ids are opaque and stable: w1, w1:t1, w1:p1. Closed ids are never reused. Every
managed pane gets DIRK_WORKSPACE_ID, DIRK_TAB_ID and DIRK_PANE_ID in its
environment, and --current resolves to the calling pane.

Reads never mark an agent seen. That rule belongs in the protocol, not in the
callers.

Depends on the server (0007): there is no socket without a daemon.

## Acceptance criteria
- [x] The noun/verb surface, returning JSON — less `agent start`, which needs
      dirk to know how to launch each agent kind rather than just recognise
      one; split out as 0061
- [x] Caller context injected into every managed pane
- [x] --current targets the calling pane
- [x] A schema command, so the surface is discoverable rather than guessed
- [x] Refuses to drive a session that is not running, rather than starting one
      to answer. Addressing *another* of your own sessions with `--session` is
      deliberate; the socket directory is 0700, so the boundary is the user.
- [x] Errors are structured, not prose on stderr
