---
id: 118
title: 'pane wait-output: the same, for things that are not agents'
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: api
effort: m
---

## Problem
Half of what runs in a pane is not an agent: a test watcher, a dev server, a
deploy. `agent wait` cannot help there, and there is nothing else.

## Proposal
`dirk pane wait-output <pane> --regex 'passed|failed'` searches the pane's recent
output and returns when it matches. It searches immediately, so text already on
the screen matches — a caller that starts the command and then waits should not
lose the race.

`--lines` bounds how far back "recent" reaches, defaulting to a screenful.
`--regex` uses Rust's regular expressions and matches one line at a time, so a
pattern cannot be made to backtrack across the whole scrollback.

This does not interpret agent lifecycle and should not try to. It is a grep with
a blocking read.

## Acceptance criteria
- [ ] `--regex` and a plain substring form, matched per line
- [ ] Text already present matches, without waiting for new output
- [ ] `--lines` bounds the window; a sensible default when it is absent
- [ ] The answer carries the matched line and the snapshot it came from
- [ ] The pane closing ends the wait with an error naming that
