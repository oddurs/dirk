---
id: 147
title: Saved machines, in the nav
type: feature
status: backlog
milestone: v0.10
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: nav
effort: xl
---

## Problem
`--remote` attaches to one machine and that is the session you are in. Work
spread across a laptop and two build boxes means three terminals, three sessions
and no way to see from one of them that an agent on another is blocked — which
is precisely the situation the nav was built to fix, one level up.

## Proposal
Saved machines as a first-class thing the nav knows about. `dirk machine add
workbox` records one; the nav shows Local and each connected machine, each with
its own projects and workspaces underneath.

Each machine keeps its own session and its own processes. The client holds
connections to several. A machine going away marks its rows unreachable and
reconnects in the background; it must not disturb the others, because the whole
value here is that one flaky link does not cost you the session you are in.

This is herdr's flagship feature and it fits dirk's model better than it fits
theirs, because dirk already has a column ordered by what needs you. Last in the
list on purpose: it is worth nothing until everything above it is solid.

## Acceptance criteria
- [ ] `machine add | list | rename | remove | enable | disable`
- [ ] The nav shows Local and each machine, with projects and workspaces under each
- [ ] Connections are independent; one failing does not affect the others
- [ ] A disconnected machine is marked, and reconnects in the background
- [ ] Ids and CLI commands stay scoped to one session; switching machines in the nav does not retarget them
- [ ] Removing a machine disconnects the client and does not stop its session
