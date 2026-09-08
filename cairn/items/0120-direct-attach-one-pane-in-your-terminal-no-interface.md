---
id: 120
title: 'Direct attach: one pane in your terminal, no interface'
type: feature
status: done
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: api
effort: m
---

## Problem
Attaching gives you the whole of dirk. Sometimes what you want is the one pane
the agent is in — over ssh from a phone, or inside another multiplexer, or in a
terminal too small for a sidebar to be anything but in the way.

## Proposal
`dirk pane attach <target>` streams the pane's current rendered state and then
live output to the terminal you are sitting in, and sends your input straight
back. No nav, no rail, no key handling except the one that leaves.

Detach with the prefix and `d`, as everywhere else. The prefix twice sends a
literal one through, as everywhere else.

Only one attachment owns input and resize at a time; `--takeover` replaces the
current owner rather than failing, because the usual reason you are asking is
that the other one is a terminal you have already closed.

## Acceptance criteria
- [x] Current screen first, then live output
- [x] Input and resize go to that pane alone
- [x] Prefix-d detaches; prefix twice sends a literal prefix
- [x] Scrolling works, and typing returns to the bottom
- [x] One writer at a time, and `--takeover` to become it

## 2026-09-07

Whole screens rather than the pane's raw bytes. Teeing the pty would give
perfect fidelity and would also be a second renderer, which is the thing this
codebase already decided against once — two of them drift, the local one gets a
fix and the remote one does not. `contents_formatted` is what vt100 already
keeps, it costs nothing to ask for, and an unchanged screen is not sent at all,
so a still terminal stays still.

Scrolling falls out of that: `set_scrollback` moves the window vt100 is already
keeping, so a pane being read from the past formats exactly as one at the
bottom. The scroll depth is a property of the pane rather than of the viewer,
which it already was — an attached dirk client sees the same movement.

The bug worth recording: the reply and the frames were being written by two
different threads onto one socket. The socket thread answered "you may have
this" while the event loop had already begun posting screens, so the client read
a frame where it expected a reply and gave up. From the moment a watcher exists
the loop is the only thread that writes to it, and the acceptance goes out the
same way.

Plain PageUp and PageDown page; with any modifier they go to the program.
Taking them outright would take them off `less` and off every agent's
transcript, which is a worse trade than not having them here.

CI caught one this suite could not: `crossterm::terminal::size()` fails when
there is no terminal, and `cargo test` gives a command pipes rather than a tty.
So `pane attach p9999` answered with an errno from an ioctl instead of "no such
pane" — the refusal a caller most needs to read, replaced by the least readable
thing available. The size is a hint; a missing one is not a reason to fail
before the question has even been asked.
