---
id: 183
title: Every enum setting is a string, checked twice
type: chore
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p3
area: config
effort: m
---

## Problem

`nav.rows`, `nav.attention`, `nav.glyphs`, `ui.pane_rules`, `terminal.shell_mode`,
`identity.session`, `identity.host` and the `sound.agents` values are each a
`String` with a fixed set of meanings. Each one is written down twice:

- in the accessor, as a `match self.x.as_str()` whose arms are the valid values;
- in `complaints()`, as a list of the valid values, to warn about a typo.

Two copies of one list, in two files' worth of distance, with nothing tying
them together. `0142` is the evidence: adding `rows = "intent"` meant editing
the accessor and then remembering the validator. Forgetting the second is
silent — the setting works, and a misspelling of it warns about nothing.

## Proposal

Keep the behaviour, which is deliberate and worth keeping: an unrecognised
value warns and falls back rather than refusing the whole file, because a
config that will not load is a dirk that will not start.

What changes is where the list lives. One type — a small `enum` with a
`Deserialize` that falls back and remembers that it did, or a `Choice` wrapper
carrying its own `VALUES` — puts the values, the parse and the complaint in one
place. `complaints()` then asks the value whether it was understood instead of
holding its own copy of the answer.

## Acceptance criteria

- [ ] Adding a value to a setting means editing one place
- [ ] An unrecognised value still warns and still falls back
- [ ] A test that a new value cannot be added to the accessor alone
