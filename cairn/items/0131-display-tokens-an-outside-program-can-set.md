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
- [ ] `pane metadata --set` and `--clear`, with a source name per writer
- [ ] Tokens are available to naming templates and nav rows
- [ ] Setting one never affects state, ordering or notifications
- [ ] Tokens are ephemeral: they do not survive a cold restart
- [ ] Documented beside `agent state`, saying plainly which one to reach for
