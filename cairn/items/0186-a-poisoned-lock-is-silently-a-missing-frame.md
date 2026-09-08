---
id: 186
title: A poisoned lock is silently a missing frame
type: chore
status: backlog
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

- [ ] A poisoned pane lock is visible on the row, not just absent from it
- [ ] A panicking reader thread does not take down the session
- [ ] A test that poisons a pane's lock and asserts what the nav says
