---
id: 62
title: A CLI answer through a closed pipe should not panic
type: bug
status: done
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: server
---

## Problem
`dirk session list | head -3` panics: `failed printing to stdout: Broken pipe`.
Every CLI answer goes through `println!`, which panics rather than returning the
error, so piping any of them into something that stops reading -- `head`, a
closed pager, a `grep -q` -- ends in a backtrace instead of an exit.

Restoring SIG_DFL for SIGPIPE globally is the usual fix and is wrong here: the
server writes to panes and sockets, and the client writes to an ssh pipe, where
EPIPE has to come back as an error rather than kill the process.

## Acceptance criteria
- [ ] `dirk session list | head -1` exits quietly
- [ ] `dirk pane list | head -1` exits quietly
- [ ] The server and the client still see EPIPE as an error, not a signal
