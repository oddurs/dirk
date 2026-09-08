---
id: 53
title: Clicking a notification should focus the workspace it is about
type: feature
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p3
area: packaging
effort: m
---

## Problem
Notifications say which workspace wants attention and cannot take you there.
Split out of 0033, where it was an acceptance criterion that turned out to be a
packaging question rather than a line of code.

## Why it is not simple
macOS delivers notifications through `osascript`, which has no way to route a
click back to a program that is not an app bundle. Linux's `notify-send` can
carry actions, but only with a notification daemon that supports them and a
process alive to receive the reply.

## Proposal
Decide it alongside packaging (0018). An app bundle on macOS would answer this
and the dock icon question together; a helper binary is the alternative.

## Acceptance criteria
- [ ] Clicking a notification focuses the workspace it names, on macos
- [ ] The same on linux where the daemon supports actions
- [ ] Where neither works, the notification is still delivered and says so
      nowhere -- a dead action is worse than none
