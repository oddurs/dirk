---
id: 194
title: A terminal that says it is zero by zero panics the pane's parser
type: bug
status: backlog
milestone: v0.10
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: s
area: pty
---

## What happens

Start dirk on a pty whose window size was never set, so `TIOCGWINSZ` reports
zero rows and zero columns. That is what `pty.fork()` in Python gives you, and
what some CI runners and `script` wrappers give you. The first time the
program in a pane draws, the reader thread panics:

    thread '<unnamed>' panicked at vt100-0.16.2/src/grid.rs:689:18:
    called `Option::unwrap()` on a `None` value

The pane's mutex is poisoned from then on, which dirk reads as a pane that has
ended. The chrome keeps drawing; the pane is dead.

## What should happen

A size of zero is a terminal that has not said yet, not a terminal that is
zero wide. Panes should be built at some sane minimum — or at the configured
detached size, which already exists for a session nobody is attached to — and
resized when the terminal reports a real size, the way a resize does today.

## Reproduction

    python3 -c '
    import os, pty, time
    pid, fd = pty.fork()
    if pid == 0: os.execvp("dirk", ["dirk", "--no-session"])
    time.sleep(1); print(os.read(fd, 65536))'

## Acceptance criteria

- [ ] No pane is ever handed a grid smaller than one row by two columns
- [ ] A terminal that reports zero gets the detached size until it reports
      something else
- [ ] The reader thread never panics on what vt100 does with a size
