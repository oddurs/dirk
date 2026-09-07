---
id: 63
title: Nothing removes a socket its server has left behind
type: feature
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: server
---

## Problem
A server that dies without unbinding leaves its socket, and only a new session
of the same name ever removes one.  `dirk session list` reports them as stale,
which is the right answer, but the list is mostly stale entries after a while
and there is no way to clear them.

## Acceptance criteria
- [ ] A way to remove sockets nothing is listening on
- [ ] A socket that is still answering is never removed
