---
id: 138
title: The title dirk writes to the terminal it runs in
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: chrome
effort: s
---

## Problem
dirk emulates its panes' terminals, so a title written inside a pane stops at
dirk and dirk writes nothing of its own. Every window manager, terminal tab bar
and application switcher reads that title, so the window holding a session of
fourteen panes is labelled whatever it was called before dirk started.

## Proposal
`[ui] window_title = "{host}: {workspace}"`, with tokens for the host, the
workspace, the tab and the focused pane's own title with spinner frames
stripped. Written from the session end, so `--remote` names the machine the work
is on rather than the laptop it is displayed on — the opposite of the rule for
sound and notifications, and for the same reason: this one describes the work,
those describe where you are.

Empty leaves the outer title alone.

## Acceptance criteria
- [x] Tokens for host, workspace, tab and pane title; `{{` and `}}` escape
- [x] A token with no value renders empty rather than as its own name
- [x] Rendered on the session end, so `--remote` names the far machine
- [x] Empty leaves the title untouched
- [x] Restored on exit, including on an abnormal one

## 2026-09-08

Restored through the terminal's own title stack — `ESC[22;2t` on the way in and
`ESC[23;2t` on the way out — because there is no way to ask a terminal what its
title is, and the stack is the only mechanism that puts one back. A terminal
without it keeps dirk's, which is what happened before dirk wrote one at all.
The pop is in `client::restore`, so it runs on a panic too.

Composed by the session and written by the client, like sound and the desktop
notification: the facts are the session's — `{host}` is the machine the panes
are on — and the terminal belongs to whoever is looking at it.

Writing it turned up that `run`, the loop for `--no-session`, had drifted from
`turn`: it was doing none of the work the session loop had gained since they
were written. Both now call one `after_events`, which is why this change is
larger than a title.
