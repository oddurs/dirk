---
id: 148
title: Getting dirk onto the far side of an ssh connection
type: feature
status: backlog
milestone: v0.10
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: server
effort: l
---

## Problem
`--remote` needs a compatible dirk already on the far machine. So the first step
of using it is a manual install on every box you might want to reach, which for
a fresh container or a box you were handed an hour ago is most of them.

## Proposal
Check the far side: its platform and architecture, then a `dirk` on `PATH`, then
the usual install locations. If nothing compatible is there, offer to install
one under `~/.local/bin` — copying the local binary when the platforms match,
downloading the matching release asset otherwise.

Interactive runs ask. Non-interactive runs fail rather than writing to somebody's
machine because a script assumed yes. `DIRK_REMOTE_BINARY` names a local build,
for testing a change against a real remote before it is released.

## Acceptance criteria
- [ ] Platform and architecture detected before anything is transferred
- [ ] `PATH` first, then the usual install locations
- [ ] Interactive runs ask; non-interactive runs fail without modifying the host
- [ ] Local binary copied when platforms match, release asset downloaded otherwise
- [ ] A warning when the install directory is not on the remote PATH
- [ ] `DIRK_REMOTE_BINARY` overrides with a local file
