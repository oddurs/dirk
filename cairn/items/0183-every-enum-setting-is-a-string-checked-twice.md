---
id: 183
title: Every enum setting is a string, checked twice
type: chore
status: done
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

- [x] Adding a value to a setting means editing one place
- [x] An unrecognised value still warns and still falls back
- [x] A test that a new value cannot be added to the accessor alone

## 2026-09-08

A `choice!` macro rather than a wrapper type, because the field has to stay a
`String`: a typed field makes serde reject the whole document, and the whole
point of the behaviour being kept is that a typo in one setting must not cost
somebody the dirk they were about to start.

Seven of them — `nav.rows`, `nav.attention`, `ui.pane_rules`,
`terminal.shell_mode`, `identity.session`, `identity.host` and the
`sound.agents` values. The last was already an enum with a hand-written parser
beside it, so it went in too and lost the parser.

The fallback is derived from the same list rather than assumed to be the first
word in it, because for `sound.agents` it is not — `default` sits last and is
what an unrecognised value means. That was very nearly a complaint saying
"using on".

The complaint is one sentence for all of them now, which changed the wording,
which broke two tests that were asserting on the wording rather than on what was
said. Both now ask for the three things that matter: which setting, what it
could not read, and what it is going to do instead.

The test the item asked for is `every_word_a_setting_offers_is_accepted`: it
walks every setting and every word, and fails if dirk offers a value and then
complains about it. Its companion checks the other direction. Both read from one
list of settings, so the failure they cannot catch — a setting added to
`choice!` and to nothing else — is the one the list itself is a comment about.
