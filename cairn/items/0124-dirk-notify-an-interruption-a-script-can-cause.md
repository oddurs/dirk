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
- [x] Title and optional body, delivered client-side like agent notifications
- [x] The quiet rules apply unchanged
- [x] Works from inside a pane with `--current` and from outside with an id

## 2026-09-07

`session notify` rather than a `notify` noun: the surface is nouns and verbs,
and this is asking the session to interrupt somebody. The target comes first
like every other pane and workspace command, so `--current` works from inside a
pane and an id works from outside.

No separate body. The wire alert grew a `text` instead: agent alerts leave it
empty and the client composes "<label> has finished" from the state, because the
wording belongs on the machine that shows it. A script's own words are not
something to compose around — appending "has finished" to "the deploy failed"
would be worse than saying nothing.

A suppressed notification answers `notified: false` with a reason rather than
failing. The caller did nothing wrong; the rules did their job, and a script
that treated silence as an error would learn to work around the rules.

`--blocked` picks the interrupting sound. Two events are worth hearing and they
have to be distinguishable with your back to the screen, which is the whole
design of dirk's notifications; a script that needs an answer and one that
finished are those same two events.
