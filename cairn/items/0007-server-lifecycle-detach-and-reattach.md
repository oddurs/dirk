---
id: 7
title: 'Server lifecycle: detach and reattach'
type: feature
status: done
milestone: v0.4
assignee: oddurs
created: 2026-09-06
updated: 2026-09-07
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
- [x] Sessions survive closing the terminal — the ssh half is 0039
- [x] Reattach restores focus, sizes and scrollback — the state never left
- [x] Resize on reattach propagates to every visible pane
- [x] Orphaned daemons are reaped rather than accumulating — a server whose socket has gone exits, since nothing can reach it
- [x] A second client attaching to one session does not fight the first — it
      takes over, and the first is told why. A view per client needs a focus per
      client, which is a change to what a session *is*; split out as 0060.

## Plan

**The server renders; the client paints opaque bytes.**

That is the decision everything else follows from, and the alternative — ship
session state and let the client compose it — is the one that looks more
principled and costs more. Two renderers drift: the local one gets a fix, the
remote one does not, and the difference shows up as a rendering bug nobody can
reproduce. There is one renderer, it lives in the server, and a client is a
terminal with a socket.

It also means the frame protocol needs no design at all. ratatui already diffs
one buffer against the next and emits the minimal escape sequences; pointing its
backend at a `Vec<u8>` instead of a tty turns that into bytes to post. The client
writes them out. Remote attach is then the same code path as local, which is why
0039 is small.

**The client parses input, the server consumes it.** crossterm's event parser is
the client's job because that is where the terminal is. Events cross the wire as
a small enum and are turned back into crossterm events on the far side, so
`App::handle` is untouched by any of this.

**Framing** is a length prefix and a kind byte: JSON for control, raw bytes for
frames. Base64 in JSON would inflate every frame by a third for nothing, and a
line-delimited protocol cannot carry escape sequences without escaping them
twice.

**The daemon is dirk re-executing itself** with `server`, in its own session via
`setsid` in `pre_exec` — the textbook shape, and it avoids forking a process that
already has a thread per pane.

**One client at a time.** A second attach takes the session over and the first is
told why. Per-client views are a real feature and a different one: they need a
`Terminal` and a focus per client, which is a change to what a session *is*
rather than to how it is reached.
