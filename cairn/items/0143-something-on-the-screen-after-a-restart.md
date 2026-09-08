---
id: 143
title: Something on the screen after a restart
type: feature
status: done
milestone: v0.9
created: 2026-09-07
updated: 2026-09-08
priority: p1
area: server
effort: m
---

## Problem
A restored session brings back the shape and a set of empty shells. Even where
the process cannot come back, what it last said usually still matters — the
error you were reading, the test summary, the output you had not copied yet.

## Proposal
Persist the tail of each pane's rendered screen alongside the session file, and
paint it into the restored pane before the new shell's first prompt. Marked as
history rather than live, because a screen showing a build that finished
yesterday should not be mistaken for one running now.

Off by default, and staying off by default. Pane output holds tokens, keys and
whatever was in the environment when something printed a debug line; writing
that to disk is a decision the person running it should make deliberately. The
documentation says so where the setting is, not in a footnote.

## Acceptance criteria
- [x] `[session] pane_history = false` by default
- [x] A bounded tail per pane, written beside the session file
- [x] Restored panes paint it before the new shell starts
- [x] The rail or the pane marks it as history, not live output
- [x] Turning it off deletes what was already stored
- [x] The security trade-off documented at the setting
