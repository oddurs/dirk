---
id: 186
title: A poisoned lock is silently a missing frame
type: chore
status: done
created: 2026-09-08
updated: 2026-09-08
priority: p3
area: mux
effort: s
---

## Problem

Every `.lock()` in the tree handles failure, which is good discipline — there
is not one `lock().unwrap()` in `src/`. They all handle it the same way:

    let Ok(mut term) = pane.term.lock() else { return; };
    let Ok(mut store) = pane.graphics.lock() else { continue; };
    Err(_) => continue,

A `Mutex` fails to lock for one reason: a thread panicked while holding it. So
every one of these reads as "if this pane's reader thread has died, quietly do
nothing" — and quietly is the problem. The pane stops updating, stays on
screen, keeps its row in the nav, and says nothing about why. It looks exactly
like a program that has gone quiet.

It is uniform, and it is uniformly silent. Twenty-five sites, no diagnostic at
any of them.

## Proposal

Not `unwrap` — a panicked reader should not take the session down with it. What
is missing is that the pane knows.

A poisoned `term` means the pane is unreadable from now on, which is a fact
about the pane and belongs on it: mark it dead the way an exited program is,
say so on the row, and let the existing path for a pane whose program has gone
do the rest. One place decides, and the twenty-five sites keep doing what they
do.

## Acceptance criteria

- [x] A poisoned pane lock is visible on the row, not just absent from it
- [x] A panicking reader thread does not take down the session
- [x] A test that poisons a pane's lock and asserts what the nav says

## 2026-09-08

Not `unwrap`, and not a check at each of the twenty-five sites either. Those
sites are right to do nothing: none of them owns the pane, and none of them can
tell a lock that is busy from one that is broken without asking a different
question than the one it is asking. So one sweep owns it, and it runs at the top
of `after_events` — before anything reads a grid, because a pane read once more
after it stopped being readable is a pane drawn from a frozen copy of itself.

The pane is ended rather than merely flagged. Nothing it writes can reach a
screen from here on, so leaving the program running leaves it talking into a
closed pipe — the same argument `finish` already makes about a pty whose far end
has gone, and the same treatment, with a different sentence in `exit` because
this is not how a program usually ends.

The last criterion said "what the nav says" and this asserts what the *pane*
says, which is the thing the nav reads. Poisoning a lock means panicking a
thread that holds it, and that can only be done from inside the process — so
the test builds a real pane, poisons its grid on purpose, and checks that it is
buried and says why. The panic hook is swapped for a silent one first: the panic
is the point of the test and its backtrace is not.

A session-level test would need a pane whose reader falls over on its own, which
is the bug this is about and not something a test can ask for.
