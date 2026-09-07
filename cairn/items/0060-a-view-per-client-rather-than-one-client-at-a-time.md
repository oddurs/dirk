---
id: 60
title: A view per client, rather than one client at a time
type: feature
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: server
effort: l
---

## Problem
A second client takes the session over and the first is told why. That is honest
but it is not what tmux does, and it rules out the case the feature is most
wanted for: the same session on a laptop and on a monitor, looking at different
parts of it.

## Why it was not done in 0007
A view per client is not a transport change. Each client needs its own
`Terminal`, its own size, and its own **focus** — which means focus stops being
a property of the session and becomes one of a viewer. Everything that reads
`session.focus` today would have to say whose focus it means: the nav
highlight, the seen rule that decides `done` from `idle`, naming's
multi-pane check, and the rail.

That is a change to what a session is, not to how it is reached.

## Acceptance criteria
- [ ] Several clients attached at once, each with its own size
- [ ] Focus is per client; the nav highlights each viewer's own
- [ ] The seen rule reads "seen by anyone", so one viewer clears `done`
- [ ] A pane resizes to the smallest client showing it, as tmux does
- [ ] Detaching one client does not disturb another
