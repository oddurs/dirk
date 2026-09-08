---
id: 70
title: A sound when an agent blocks, and one when it is done
type: feature
status: done
milestone: v0.5
depends_on:
- 68
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: agents
effort: m
---

## Problem
There is no sound at all.  The entire reason to run agents in parallel is to be
doing something else while they work, and the state you most need is the one you
are not looking at.

## Transitions, not states
There are exactly two events worth hearing: an agent entering `blocked` and an
agent entering `done`.  `working` and `idle` are conditions, not news.

The two must be distinguishable with your back to the screen, because that is
the whole point -- one interrupts and one satisfies.  Both short, both quiet.

Everything else here is a rule about when to stay silent, and those rules decide
whether this is loved or muted within a week.

* **Silent when you are looking at it.**  If the workspace that just finished is
  the focused one, you already know.  This is the most important rule here.
* **Floored, per workspace.**  Reuse `notify.min_interval_ms`.  An agent that
  blocks, unblocks and blocks again inside a minute is one interruption, which
  the notification path already agrees with.
* **Mutable per project.**  The repo you are babysitting should be able to be
  quiet without silencing the one you are not.

## An alert belongs to the client, and today it does not
This one matters more since 0.4.  `notify.rs` runs `osascript` **in the server**.
Attach over ssh and the session is on the build box while the human is at the
laptop, so the notification appears on a machine nobody is sitting at.  Sound
would inherit exactly the same defect, and a speaker command is even more
obviously wrong on the wrong machine.

So alerts travel: the server decides and the client performs.  A `wire::Kind`
for it, handled next to `Frame` and `Bye`, and `notify.rs` moves behind the same
door.  The bell fallback can ride in the frame bytes -- it is a byte the client
paints like any other -- but a command has to be run where the speakers are.

## The bell runs both ways
If 0068 reads a pane's bell as evidence, dirk must never answer with an alert
bell into a pane: a done sound becomes a done signal becomes a done sound.  The
alert bell goes to the client's terminal; a pane's bell is consumed and never
re-emitted.

## Shape
`src/sound.rs`, mirroring `notify.rs`: a command per event, spawned off the
drawing thread, defaulting to `afplay` on macOS and `paplay` on Linux, falling
back to the terminal bell where neither exists.  No audio crate -- dirk already
shells out for the process table, git, notifications and naming, and this is the
same kind of thing.

The insertion point is `Session::update_states() -> Vec<Change>`, which already
exists and already drives notifications.  Sound hangs off the same stream, which
is what keeps the two from disagreeing about what happened.

## Acceptance criteria
- [ ] A sound on entering blocked and a different one on entering done
- [ ] Silent when the workspace in question is the focused one
- [ ] Floored per workspace, reusing `notify.min_interval_ms`
- [ ] `[[project]] sound = false` mutes one project; `[sound] enabled` mutes all
- [ ] A command per event, no audio dependency, never on the drawing thread
- [ ] Alerts are performed by the client, so a remote attach makes noise where
      the human is -- and `notify.rs` moves to the same path
- [ ] A pane's bell is consumed as evidence and never answered with an alert bell
- [ ] Absent player, absent file, or a failed command degrade silently
