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

- [x] A client that dies while waiting releases the thread, the socket and the
      held question
- [x] `App::waits` does not grow across abandoned calls
- [x] A test that abandons a wait and asserts the session lets go of it

## 2026-09-08

Both ends, as the item guessed, and neither is enough alone.

The socket thread waits in steps now instead of one indefinite `recv`, and
between them asks the connection whether the far end is still open — one
`recv` with `MSG_PEEK | MSG_DONTWAIT`, peeked rather than read because anything
already there is the caller's next request and the connection stays open for
more. `UnixStream::peek` would have said it in one line and is still unstable;
a read timeout would have meant putting the socket's timeout back before the
blocking read that follows, and forgetting to would end that read early.

The session end needed something the sender could not tell it. A `SyncSender`
learns its receiver has gone only by sending, and a question with no answer yet
has nothing to send — so a caller that walked away was invisible. `wire::Answer`
carries a token instead: the caller holds one end of it for exactly as long as
it is waiting, and holding the last reference is what "nobody wants this" means.
`settle()` asks that before it decides anything, because deciding a question for
nobody is work done on every turn of the loop for the life of the session.

`session info` reports `waiting` now. It is the same kind of fact as the other
counts, it is the first thing to look at when a script seems hung, and it is
what made this testable: the test starts a wait as its own process, kills it,
and watches the count go back to zero. A thread on `Command::output` cannot be
abandoned, so only a process would do.

Measured the way it was found. Five clients waiting take the session from four
threads to nine; four seconds after killing all five it is back to four.

    before:                 4
    while 5 wait:           9
    4s after killing them:  4

And checked the other way round: with the `settle()` guard disabled the test
fails, so it is testing the fix rather than the weather.
