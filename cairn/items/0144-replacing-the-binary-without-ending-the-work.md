---
id: 144
title: Replacing the binary without ending the work
type: feature
status: done
milestone: v0.9
created: 2026-09-07
updated: 2026-09-08
priority: p2
area: server
effort: xl
---

## Problem
Upgrading means stopping the session, which means ending every shell and every
agent. So people do not upgrade while they are working, which is always. A
long-running session is exactly the thing that makes an upgrade expensive, and
exactly the thing dirk is for.

## Proposal
Hand the running ptys to the replacement server across an exec: the old server
passes pty file descriptors, pane identity, agent state and the durable session
over a socket, and the new one adopts them. Processes never stop.

Be precise about what does not survive. In-flight requests, outstanding waits,
subscriptions and client connections are all cut; clients reconnect and retry.
That boundary must be documented, not discovered, because a caller waiting on an
agent through a handoff needs to know its wait ended rather than silently never
returning.

Opt-in while it is new. This is the largest and riskiest item in the milestone
and it should behave that way.

## Acceptance criteria
- [x] Pty file descriptors pass to the replacement server; processes keep running
- [x] Pane identity, agent state and session state come across
- [x] Mouse mode, bracketed paste and terminal modes are preserved per pane
- [x] A failed handoff leaves the old server running rather than losing the panes
- [x] What is cut is documented; waits end with an error naming the handoff
- [x] Opt-in, and marked experimental
