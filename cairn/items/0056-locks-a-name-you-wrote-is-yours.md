---
id: 56
title: 'Locks: a name you wrote is yours'
type: feature
status: done
milestone: v0.3.1
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: naming
effort: m
---

## Problem
The policy already refuses to overwrite a hand-written name — but it works it
out afresh every time by comparing the live label against the last one dirk
wrote. namesync keeps an explicit lock, which is what makes it possible to say
so in the interface and to take it back.

Clearing a name by hand should hand the workspace back. namesync learned that
one the hard way: a permanent hold with no visible release leaves a workspace
blank for ever, because nobody knows the command that clearing was supposed to
mean.

## Acceptance criteria
- [x] A hand-written name is held explicitly, not inferred each pass
- [x] The `locked` token, and a mark in the nav
- [x] An action to release one, reachable without knowing a command
- [x] Clearing a name by hand releases the hold rather than freezing it blank
- [x] Releasing a hold renames on the next intent, not immediately
