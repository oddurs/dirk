---
id: 39
title: Attach over ssh
type: feature
status: done
milestone: v0.4
depends_on:
- 7
created: 2026-09-06
updated: 2026-09-07
priority: p3
area: server
effort: l
---

## Problem
Work on a remote machine means running dirk there and looking at it through
whatever terminal got you in, which loses the local terminal's capabilities.

## Acceptance criteria
- [x] dirk --remote <target> attaches to a remote daemon
- [x] Local keybindings by default, with the server's on request
- [x] Reconnects after a dropped connection without losing the session

## Plan
The README already says a client is a terminal with a socket, so this is a
transport change, not a second implementation.

1. **The client stops naming its transport.** `pump` is written against a
   reader and a writer rather than a `UnixStream`. A Unix socket has to be
   cloned to get two halves; a child process hands them over already separate,
   which is the easier case.
2. **`dirk relay`** on the far side: connect to the session socket, starting it
   if it is not there, and copy bytes both ways between it and stdin/stdout.
   Every write is flushed, because the wire is binary and a `LineWriter` would
   hold a frame until something in it happened to be a newline.
3. **`dirk --remote <target>`** runs `ssh -T -- <target> dirk --session <name>
   relay` and drives the ordinary client over the pipe pair. `-T` because a pty
   would put a line discipline between two programs speaking a binary protocol.
   `$DIRK_REMOTE` names the far-side binary when it is not on `PATH`.
4. **Reconnect.** One input thread for the life of the process, writing to
   whichever link is current; keys pressed while there is no link are dropped
   rather than replayed, because they belonged to that moment. A connection
   that never delivered a frame is not retried -- that is a configuration
   problem wearing a network problem's clothes -- and ssh's own stderr is what
   gets reported.

Local keybindings already win by default: when you ssh out of a pane the outer
dirk sees the prefix first, and `Ctrl-Space Ctrl-Space` passes a literal one
through to whatever is inside, including a remote dirk.

## Not doing
Reattaching the *session* over ssh (`dirk --remote` starts nothing locally), a
config file section (an environment variable is enough for a path), and any
transport of our own: ssh is the authentication story.
