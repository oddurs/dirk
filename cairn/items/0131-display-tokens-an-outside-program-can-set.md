---
id: 131
title: Display tokens an outside program can set
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: agents
effort: m
---

## Problem
`dirk agent state` lets a program report lifecycle state, which is right: state
drives notifications, ordering and the attention column, and it must stay a
small closed set that dirk reasons about.

But that is also the only channel, so anything a program wants to *show* — an
indexer's progress, a test count, which model is running — has nowhere to go
except into a state it will be reasoned about with.

## Proposal
Separate the two. `dirk pane metadata <pane> --set summary=indexing` records
display-only tokens that never touch state, waits, notifications or rollups.
Naming already has a token and template system; these become tokens in it, so a
row can be arranged to show one without a new configuration mechanism.

## Acceptance criteria
- [x] `pane metadata <pane> key=value`, and an empty value clears — no source, see below
- [x] Tokens are available to naming templates and nav rows
- [x] Setting one never affects state, ordering or notifications
- [x] Tokens are ephemeral: they do not survive a cold restart
- [x] Documented beside `agent state`, saying plainly which one to reach for

## 2026-09-08

Namespaced as `{said.summary}` rather than flat. dirk checks its own token names
against a list and reports a typo, and it cannot check somebody else's — so the
two vocabularies are kept apart, which also means nobody's `summary` can shadow
a token dirk adds later.

No `--source`. herdr has one so that several writers can each own their tokens;
dirk's are already scoped to a pane, and a source would double every key to
solve a problem nobody in one session has. Last writer wins, and if two programs
are writing the same token about the same pane they have a bigger disagreement
than this could arbitrate.

"nav rows" is met through naming: a template is what a row shows, and templates
are where tokens are addressed. What that turned up is that a template only runs
when naming has an intent to decide a label from — so the test has to give the
pane a title before the token can appear, which is worth knowing.
