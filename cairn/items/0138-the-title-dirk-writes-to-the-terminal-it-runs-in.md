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
- [ ] Tokens for host, workspace, tab and pane title; `{{` and `}}` escape
- [ ] A token with no value renders empty rather than as its own name
- [ ] Rendered on the session end, so `--remote` names the far machine
- [ ] Empty leaves the title untouched
- [ ] Restored on exit, including on an abnormal one
