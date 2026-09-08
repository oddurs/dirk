---
id: 146
title: Watching and driving a terminal over the socket
type: feature
status: backlog
milestone: v0.9
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: api
effort: m
---

## Problem
`pane read` returns text at a moment. Anything that wants to follow a pane has
to poll, and anything that wants to render one — a bridge, a web client, a
recorder — cannot: the escapes are stripped, and there is no stream.

## Proposal
Two streams over the existing socket. `pane observe <target>` writes
newline-delimited JSON frames carrying base64 terminal bytes, then a closing
record; any number of observers can watch one pane without taking input, resize
or scroll from anybody.

`pane control <target>` is the same stream plus commands read from standard
input: input, resize, scroll, release. One controller at a time, `--takeover` to
become it.

This is the honest version of a plugin API. Somebody who wants to build a
different front end for dirk needs bytes out and bytes in, not a manifest
format.

## Acceptance criteria
- [ ] `pane observe` streams frames; many observers, no ownership
- [ ] `pane control` accepts input, resize, scroll and release on stdin
- [ ] One controller at a time; `--takeover` replaces it
- [ ] A closing record when the pane ends, distinguishable from a dropped connection
- [ ] Backpressure handled: a slow reader is dropped, not allowed to stall the session
