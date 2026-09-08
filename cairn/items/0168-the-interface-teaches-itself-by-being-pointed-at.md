---
id: 168
title: The interface teaches itself by being pointed at
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: m
area: chrome
depends_on:
- 160
- 161
- 162
---

## Problem
Right now the interface teaches itself in one way: the footers print the keys.
That is a real answer and it is the keyboard's answer -- three rows of the
sidebar spent on a small reference card.

If pointing becomes the primary way through, those rows are being spent
teaching the secondary one. But deleting them and putting nothing there is how
a mouse-first interface becomes an interface where you have to already know
things, which is the failure this milestone exists to fix in the first place.

## Proposal
This is the item to do last, and it may turn out to be nothing.

Once hover says what a thing is, affordances say what is pressable, and
right-click lists what applies, the footers may have nothing left to teach --
or they may be the only thing on screen that says a key exists at all. Which of
those is true is a question about a working interface, not one to answer from
here.

What can be said now is the shape of the answer:

- Nothing gets deleted before the thing that replaces it exists.
- Whatever goes there says what dirk can do, not what dirk is.
- If the honest answer is that the footers were right all along, that is a
  result and it gets written down rather than quietly ignored.

The measure is somebody who has never seen dirk finding their way to starting
an agent in a worktree without reading anything -- which is the thing the
milestone is actually for, and the only item here that checks it end to end.

## Acceptance criteria
- [ ] Somebody who has not read the manual can start an agent in a worktree by pointing
- [ ] Nothing that taught something was removed before its replacement worked
- [ ] The keys are still discoverable on screen, wherever that ends up being
- [ ] The conclusion is written down, including if it is "leave it as it was"
