---
id: 39
title: Attach over ssh
type: feature
status: backlog
milestone: v0.4
depends_on:
- 7
created: 2026-09-06
updated: 2026-09-06
priority: p3
area: server
effort: l
---

## Problem
Work on a remote machine means running dirk there and looking at it through
whatever terminal got you in, which loses the local terminal's capabilities.

## Acceptance criteria
- [ ] dirk --remote <target> attaches to a remote daemon
- [ ] Local keybindings by default, with the server's on request
- [ ] Reconnects after a dropped connection without losing the session
