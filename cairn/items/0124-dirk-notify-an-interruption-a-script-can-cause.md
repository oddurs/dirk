---
id: 124
title: 'dirk notify: an interruption a script can cause'
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: api
effort: s
---

## Problem
dirk knows how to make a noise where you are sitting and how to put a desktop
notification on the machine you are at rather than the one the session is on.
Only dirk's own agent state machine can ask for one.

## Proposal
`dirk notify <title> [body]` puts a build, a deploy or a cron job through the
same path, and therefore through the same rules: nothing about the workspace you
are looking at, not more often than the floor, nothing for a project that asked
to be quiet.

## Acceptance criteria
- [ ] Title and optional body, delivered client-side like agent notifications
- [ ] The quiet rules apply unchanged
- [ ] Works from inside a pane with `--current` and from outside with an id
