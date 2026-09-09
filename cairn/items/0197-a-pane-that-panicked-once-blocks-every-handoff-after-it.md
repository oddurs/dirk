---
id: 197
title: A pane that panicked once blocks every handoff after it
type: bug
status: backlog
milestone: v0.10
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: s
area: server
---

## What happens

`session handoff` walks every pane and captures its screen. Capturing takes the
pane's `term` lock, and a lock whose holder panicked is poisoned for ever, so
the capture returns `pane N is locked` and the handoff is refused. It is
refused every time after that: nothing clears a poisoned lock, and the pane
does not have to be alive for the walk to reach it.

One pane that panicked once therefore ends the dev loop for the life of the
session. `make dev` keeps building and every handoff is refused, which reads as
the watcher having stopped working.

Seen for real: a client attached at zero by zero panicked that pane's parser
(item 0194), the pane died, and every handoff afterwards said `pane 2 is
locked`. `workspace close` on the workspace holding it answered
`{"closed": "w3"}` and the pane was still in `pane list` — so there was no way
back short of ending the session.

## What should happen

A dead pane has no screen worth carrying across an `exec`, and a poisoned lock
is a pane that has already lost its screen. Neither should be able to refuse a
handoff: skip the pane, keep its descriptor if it has one, and say in the note
which panes did not come across. The handoff already reports what does not
survive rather than pretending everything does.

Whether `workspace close` should really leave the pane behind is the second
half of this — either it removes it or it does not say it closed.

## Acceptance criteria

- [ ] A handoff succeeds with a dead pane in the session
- [ ] A handoff succeeds with a pane whose `term` lock is poisoned
- [ ] Closing a workspace either removes its panes or does not answer `closed`
- [ ] A test proves the first two through the real binary

## Notes

Found while verifying `make dev` (0196) end to end. Related: 0194, which is how
the lock got poisoned, and #110, which fixed a poisoned lock silently becoming
a missing frame — the same lock, a different consequence.
