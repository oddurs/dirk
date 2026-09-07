---
id: 68
title: An agent that reports its own state
type: feature
status: done
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: agents
effort: l
---

## Problem
Every signal dirk uses to decide what an agent is doing is inferred: argv for
which harness, the window title for intent, prose on the screen for blocked,
silence for done.  Inference already needed one save -- every finished turn ends
in a question, so a menu of numbered answers had to become a requirement before
a block was believed.

The agents can simply say.  Claude Code has `Stop` and `Notification` hooks;
the others are growing the same.  dirk already has the CLI, `DIRK_PANE_ID` and
`--current`, so the hook is three lines.  This is the cheapest large gain
available in the whole nav concept: it turns a badge you glance at into one you
leave the room on.

## Ranked, because a weak signal must not make a strong claim
A wrong `working` costs nothing.  A wrong `blocked` is an interruption you did
not need, and two of those is how a person turns a feature off.  So the sources
are ranked, and each is limited to what it can honestly assert.

| rank | signal | how | may claim |
|------|--------|-----|-----------|
| 1 | the agent says so | a hook runs `dirk agent state done --current` | anything |
| 2 | the terminal says so | window title, OSC 9 / 777, the bell | anything |
| 3 | the screen looks like it | a menu of two or more numbered choices | blocked, working |
| 4 | silence | no output for N seconds, having produced some | done only |

Silence may promote *working → done*.  It may never say *blocked*.

## What supersedes a report
A reported state is a claim about a moment, not a lease, so it does not expire
on a clock -- a reported `done` has to survive until you look at it or the
whole seen rule stops meaning anything.

Two things end one.  The next report, and **activity**: a pane producing output
is working, whatever it said a minute ago.  Without that rule a harness whose
hook fires on stop but not on start gets stuck reading `done` while it grinds,
which is worse than the guess it replaced.

## Starting
A fifth state, and the only new one.  Today a harness that hangs on launch is
indistinguishable from one that is idle, and those want opposite responses.
`starting` is set when a pane is spawned with a known agent argv and nothing has
been seen yet; it leaves on the first output or the first report.

## Shape
* `dirk agent state <blocked|working|done|idle> [--current|<id>]`, through the
  existing socket API -- a new verb in `api::COMMANDS`, so it lands in
  `dirk session commands` and `--skill` for free.
* `Workspace` gains `reported: Option<(State, Instant)>`.  `observe`/`apply`
  consult it first.
* The decision becomes one function over all available evidence, returning the
  state **and the rank that decided it**.  `agent list` reports both.  That
  `why` field is what makes this debuggable and, more to the point, trustable:
  when a badge is wrong you can see which signal was wrong.
* `dirk agent hooks <harness>` prints an installable snippet, guarded with
  `[ -n "$DIRK_PANE_ID" ]` so it is a no-op outside dirk and safe to leave in a
  settings file for good.
* The README documents installing one, per harness.

## Acceptance criteria
- [ ] `dirk agent state <state> --current` sets a workspace's state
- [ ] A report outranks anything inferred, and is ended only by the next report,
      by output, or by the pane going away
- [ ] Output supersedes a reported `done` and promotes to `working`
- [ ] Silence can promote working to done and can never claim blocked
- [ ] `agent list` reports the state and which rank decided it
- [ ] `dirk agent hooks claude` prints a snippet that is inert outside dirk
- [ ] A `starting` state, so a harness that hangs on launch is not "idle"
- [ ] The seen rule is unchanged: `done` still means finished and not looked at
