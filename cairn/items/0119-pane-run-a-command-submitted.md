---
id: 119
title: 'pane run: a command, submitted'
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: api
effort: s
---

## Problem
`pane send-keys` is the only way to put a command in a pane, so every caller
writes the text and then a separate Enter, and every caller gets the ordering
wrong at least once against a program that is slow to read.

## Proposal
Three verbs where there is one. `pane run <pane> <command>` writes a command and
submits it. `pane send-text` writes literal text and submits nothing.
`pane send-keys` sends named keys and chords — `esc`, `up`, `ctrl+c` — which is
what an interactive UI needs and what a shell does not.

The split matters because the failure modes differ: text can be pasted, keys
cannot, and a command needs both in a guaranteed order.

## Acceptance criteria
- [ ] `pane run` writes the command and a submitting newline as one ordered write
- [ ] `pane send-text` never submits
- [ ] `pane send-keys` accepts named keys and modifier chords, `escape` aliasing `esc`
- [ ] A pane whose program has exited is refused rather than written to
