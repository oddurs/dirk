---
id: 120
title: 'Direct attach: one pane in your terminal, no interface'
type: feature
status: backlog
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
- [ ] Current screen first, then live output
- [ ] Input and resize go to that pane alone
- [ ] Prefix-d detaches; prefix twice sends a literal prefix
- [ ] Scrolling works, and typing returns to the bottom
- [ ] One writer at a time, and `--takeover` to become it
