---
id: 127
title: Detection rules as files, replaceable one at a time
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: agents
effort: m
---

## Problem
`[[agent]]` blocks live in `config.toml`, and a block whose name matches a
shipped harness replaces it whole. That is the right rule, but it means fixing
one marker for one harness is an edit to the file that also holds your projects,
your boards and your theme — and there is no way to hand somebody that fix.

## Proposal
Split them out: `~/.config/dirk/agents/<name>.toml`, one file per harness, each
one a complete replacement for the shipped rules of that name. A local file
always wins. `[[agent]]` in `config.toml` keeps working, because it is what these
were called first.

An invalid file is ignored with a warning and the shipped rules stand. A
detection rule that fails to parse must not be able to take out the session that
was going to draw with it.

Explicitly not doing the other half of what herdr does here: fetching rule
updates from a server. A redraw whose behaviour depends on a file downloaded
overnight is not a redraw anybody can debug.

## Acceptance criteria
- [x] `~/.config/dirk/agents/<name>.toml` read at startup and on reload
- [x] A local file replaces shipped rules of that name whole
- [x] `[[agent]]` in config.toml keeps working and is documented as the older form
- [x] An invalid file warns and falls back; it never fails startup
- [x] `dirk agent rules` lists each harness and which source decided it

## 2026-09-08

The filename names the harness, so a file has no `name` field. A `name` inside
one that disagreed with the filename would leave two ways to say which harness a
document is about and no way to tell which won.

Files beat `[[agent]]` blocks, and are read in sorted order so two of them
cannot decide between themselves which was last. A file is the more specific
statement — somebody wrote a document about that one harness — and the block is
the older way of saying the same thing.

`Kind` grew a `from`, which is not part of what a rule is and is kept anyway:
when a state is wrong the first question is which rules decided it. `agent
rules` reports it, and `0128` will report it per pane.
