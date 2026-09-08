---
id: 121
title: A terminal size when nobody is looking
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: server
effort: s
---

## Problem
Panes are sized from the attached client. With no client attached there is no
size, so a session created by a script and never looked at either has no
geometry or inherits whatever the last client happened to have. Everything else
in this milestone assumes dirk can be driven with nobody watching, and this is
what that rests on.

## Proposal
A configured fallback the server uses for layout and for new panes whenever no
client is attached. When a client attaches, panes follow it as they do now; when
the last one leaves, existing panes keep their size and anything new uses the
fallback.

## Acceptance criteria
- [ ] `[server] headless_cols` and `headless_rows`, with a documented default
- [ ] Panes created with no client attached use it
- [ ] Existing panes keep their last size when the last client detaches
- [ ] Attaching resizes to the client, as now
