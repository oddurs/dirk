---
id: 121
title: A terminal size when nobody is looking
type: feature
status: review
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
- [x] `[server] headless_cols` and `headless_rows`, with a documented default
- [x] Panes created with no client attached use it
- [x] Existing panes keep their last size when the last client detaches
- [x] Attaching resizes to the client, as now

## 2026-09-07

The hardcoded 80x24 in `serve()` was only half of it. `App::content` is what
every pane-creating path is sized from, and it is written over by whichever
client last drew — so after a client attached and left, a workspace created by
a script inherited the geometry of a terminal that had gone home. The fallback
is kept as a second rect and restored when the last view goes, which is why the
test attaches an 80-column client before asking for a 200-column pane.

Panes that already exist are deliberately not resized on detach: what is drawn
in them was drawn for the size they had.
