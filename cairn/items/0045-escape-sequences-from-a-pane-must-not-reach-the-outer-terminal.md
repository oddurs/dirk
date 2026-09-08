---
id: 45
title: Escape sequences from a pane must not reach the outer terminal
type: bug
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: pty
effort: m
---

## What happens
Pane output is parsed by vt100 and re-emitted as styled cells, which is a filter
in practice but has never been treated as a security boundary. A pane's output is
untrusted: it can come from a remote host over ssh, or from a program printing a
hostile file. SECURITY.md already says so.

## What should happen
Nothing a pane writes reaches the host terminal uninterpreted. Sequences that
change host state — clipboard writes, title setting, device queries that expect
a reply, hyperlinks — are handled or dropped deliberately, never passed through.

## Reproduction
1. In a pane, print an OSC 52 clipboard-write sequence.
2. Observe whether the host clipboard changes.

Repeat for OSC 8 hyperlinks, DECRQSS and cursor-position reports. Each needs a
decided answer and a test that pins it.

## Acceptance criteria
- [ ] An audit of every sequence vt100 passes through or reports
- [ ] Clipboard writes require an explicit opt-in
- [ ] Device queries are answered by dirk, not forwarded
- [ ] A test corpus of hostile output that must not change host state
