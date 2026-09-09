---
id: 195
title: A terminal attached through a handoff is cut and does not come back
type: bug
status: done
milestone: v0.10
assignee: Oddur Sigurdsson
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: m
area: server
---

## What happens

`dirk session handoff` keeps every shell and agent, and cuts every attached
terminal. The manual says so. But a local client that is cut simply exits, and
the reason the server sent with the `Bye` never reaches the screen: it is sent
as bare bytes and the client parses the body as JSON, so it prints nothing.

You are left at a shell prompt with no word about why, and type `dirk` again.
Fine once, for an upgrade. Not fine for a development loop that hands off on
every change to the source, which is what `make dev` is.

## What should happen

A client cut by a handoff comes back on its own, as the new binary. It is
told the session handed off, restores the terminal, and `exec`s the binary it
was started from with the arguments it was started with. The listening socket
survived the exec on the server side, so the connect queues until the new
image accepts it; nothing has to be polled. The client is new code too, so a
change to the wire between the two builds is not a problem.

A client cut for any other reason still exits, and says why.

## Acceptance criteria

- [x] An attached terminal is attached again after `session handoff`, without
      typing anything
- [x] The shell in the pane has the same pid before and after
- [x] The reason on a `Bye` reaches the screen
- [x] A test proves the first two through the real binary

## 2026-09-08

A new wire kind, `Handoff`, rather than a `Bye` with a particular reason: the
two ask opposite things of a terminal. `Bye` means stop; this means come back.
The client restores the terminal and `exec`s `current_exe()` with its own
`args()`, so it is the new binary too, attached the way it was. The listening
socket survives the server's exec, so the connect queues rather than being
refused and nothing is polled.

`exec_self` moved from `main.rs` to `handoff::exec`, since both ends now do
it. The watchers' `Bye` is sent as JSON now, which is what the far end parses;
as bare bytes the reason was read as an empty string and never printed.

Found while building `make dev` (0196): the first end-to-end run handed off
fine and left the attached terminal at a shell prompt.
