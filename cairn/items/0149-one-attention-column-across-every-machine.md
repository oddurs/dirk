---
id: 149
title: One attention column across every machine
type: feature
status: backlog
milestone: v0.10
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: nav
effort: m
---

## Problem
Once several machines are connected, the nav's promise has to hold across all of
them or the feature is a directory of remote sessions. Attention flows down one
column; an agent blocked on a build box has to appear in it.

## Proposal
The attention zone merges every connected machine's agents, ordered by state as
it is now, each row naming its machine. A folded machine carries the worst state
inside it and a count, exactly as a folded project does — the mechanism exists,
this points it one level up.

Notifications keep the rule they already have: the noise is made where you are
sitting. A machine whose rows are unreachable contributes nothing to the column
rather than contributing stale states.

## Acceptance criteria
- [ ] The attention zone spans machines, ordered by state
- [ ] Rows name their machine; a folded machine rolls up worst-state and count
- [ ] Notifications from any machine arrive at the client, once
- [ ] An unreachable machine contributes nothing rather than stale state
- [ ] The zone still occupies no rows when nothing anywhere needs you
