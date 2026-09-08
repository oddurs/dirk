---
id: 118
title: 'pane wait-output: the same, for things that are not agents'
type: feature
status: done
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
- [x] `--regex` and a plain substring form, matched per line
- [x] Text already present matches, without waiting for new output
- [x] `--lines` bounds the window; a sensible default when it is absent
- [x] The answer carries the matched line and the snapshot it came from
- [x] The pane closing ends the wait with an error naming that

## 2026-09-07

Reuses the held-question machinery from `0117`; the only new parts are a second
`What` and a `Match` that is either literal text or a pattern.

Matched a line at a time. A caller writing `^` means the start of a line and a
`.` should not reach into the next line's output, and matching the snapshot
whole would make both of those wrong occasionally rather than always — which is
worse.

`regex` is a new dependency and deliberately one: the engine has no
backtracking, so a pattern arriving from a caller's shell cannot be made to take
exponential time inside the session that holds all of their panes. Hand-written,
it would be a worse engine missing exactly that property.

The bug this shook out: `wait::options` gave every flag the next word as its
value, so `--regex --timeout 30000` parsed as a pattern of `--timeout`, a
positional `30000`, and no deadline. It did not fail — it waited for ever for a
line that could not be printed. Options that take a value are now named, and an
option a wait does not recognise is refused, because on a command that waits a
misspelling is not a wrong answer, it is no answer at all.

The answer carries `matched`, not the whole snapshot: a caller that wants the
context around it reads the pane, and putting a screenful in every wait's answer
would make the common case pay for the rare one.
