---
id: 7
title: 'Server lifecycle: detach and reattach'
type: feature
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: server
---

## Problem
v1 is a single process. Close the terminal and the work goes with it, so herdr
stays installed for anything long-running. dirk cannot replace it until sessions
outlive their client.

## Proposal
Split into `dirkd` and a thin client over a Unix socket, the way herdr and tmux
both do it. The pane tree and the pty fds move to the daemon; the client owns
only the terminal, the rendering and the input.

Deliberately deferred out of v1: it is a lot of plumbing before the first
clickable pixel, and the pane tree was worth proving first.

## Acceptance criteria
- [ ] Sessions survive closing the terminal and dropping an SSH connection
- [ ] Reattach restores focus, sizes and scrollback
- [ ] Resize on reattach propagates to every visible pane
- [ ] Orphaned daemons are reaped rather than accumulating
- [ ] A second client attaching to one session does not fight the first
