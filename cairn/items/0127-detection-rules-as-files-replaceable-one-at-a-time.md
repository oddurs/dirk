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
- [ ] `~/.config/dirk/agents/<name>.toml` read at startup and on reload
- [ ] A local file replaces shipped rules of that name whole
- [ ] `[[agent]]` in config.toml keeps working and is documented as the older form
- [ ] An invalid file warns and falls back; it never fails startup
- [ ] `dirk agent rules` lists each harness and which source decided it
