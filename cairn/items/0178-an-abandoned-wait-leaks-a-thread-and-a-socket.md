---
id: 178
title: An abandoned wait leaks a thread and a socket
type: bug
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p2
area: api
effort: m
---

## What happens

`dirk pane wait-output w1:p1 <pattern>` with no `--timeout` waits indefinitely,
which is what it is for. Kill the client — Ctrl-C, or the script that called it
dying — and the session keeps the wait alive forever:

- the per-client thread in `src/server.rs` is blocked on `wait.recv()`, which
  has no timeout and no way to learn that the peer is gone;
- its socket file descriptor stays open;
- the `wait::Held` stays in `App::waits`, and `settle()` evaluates it on every
  turn of the event loop for the life of the session.

Measured on a debug build, counting threads in the server process:

    threads before:                   4
    threads while 5 clients wait:     9
    after SIGKILL to all 5 clients:   9
    six seconds later:                9

Nothing reclaims them. A long-lived session driven by a script that times out
its own calls accumulates one of each, indefinitely.

## What should happen

A wait whose caller has gone away is a wait nobody is waiting on, and it should
be dropped — the thread released, the socket closed, the held question forgotten.

## Reproduction

1. Start a session and note the server pid.
2. `dirk pane wait-output <pane> zzz-never-happens &` five times.
3. `kill -9` all five clients.
4. `ps -M <pid>` — five threads more than before, and they stay.

## Notes

Two ends to fix and either would do; both would be better.

The server end can notice: the reading half of the connection is still in
`client()`, and a caller that has gone away closes it. A read that returns EOF
while a reply is outstanding is the signal, and it can drop the sender — which
is what makes the event loop's `send` fail.

The session end can act on that: `settle()` only sends when it has a verdict, so
a dead channel is never discovered. Checking for a closed receiver on every pass
would let it drop the held question whether or not the answer ever arrives.

Related: `answer()` in `src/server.rs` is also where a slow reply blocks a
connection that could be serving other requests, since the connection is kept
open for more.

## Acceptance criteria

- [ ] A client that dies while waiting releases the thread, the socket and the
      held question
- [ ] `App::waits` does not grow across abandoned calls
- [ ] A test that abandons a wait and asserts the session lets go of it
