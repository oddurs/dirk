---
id: 181
title: The hot path runs the diagnostic path
type: chore
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p3
area: perf
effort: s
---

## Problem

`observe()` in `src/mux/session.rs` is one line:

    fn observe(ws: &Workspace, now: Instant) -> Observed {
        explain(ws, now).observed
    }

`explain` is the version "with its working kept" — it builds a `Vec` of up to
ten `(name, state, String)` signals explaining how it reached its answer, plus
a `found` and a `window`. That exists for `dirk agent explain`, which a person
runs when they want to know why a workspace reads the way it does.

`observe` throws all of it away and keeps one field. And `observe` runs from
`update_states`, once per workspace, on every turn of the event loop — which is
every batch of pane output, every key, every tick.

So on a session with twenty workspaces, every keystroke allocates and discards
somewhere up to two hundred strings, to answer a question that needed none of
them.

The event loop coalesces events before calling `after_events`, so this is per
batch rather than per byte, and dirk is not slow today. It is a cost that
scales with the thing the nav is designed for — the config comments talk about
forty workspaces — and it is paid for nothing.

## Proposal

Split the pass: the decision, and the decision with its reasoning. The obvious
shape is for `explain` to take a flag, or for the signal pushes to go through a
recorder that is a no-op on the hot path — the second keeps the two from
drifting, which is the whole reason they are one function today.

Whatever the shape, the property to keep is the one that made this a single
function in the first place: `agent explain` must describe the pass that
actually ran, not a second implementation of it.

## Acceptance criteria

- [x] `update_states` allocates no explanation strings
- [x] `agent explain` still reports every signal it reports now
- [x] The two cannot drift: there is still one pass, not two

## 2026-09-08

The recorder, which was the shape the item preferred and is the right one: a
flag would have left `explain` free to drift into a second implementation of the
decision, and an explanation of a decision that was not the one taken is worse
than no explanation.

So `pass` decides, `observe` and `explain` are the two ways in, and what varies
is what gets written down. A signal's sentence is a closure the recorder may
decline to call — the sentences were the expensive half, not the pushing.

Two more things came off the hot path with them. `found` is the screen rule's
whole working, and `window` is a copy of a corner of the terminal; both were
built and returned on every pass and read only by `agent explain`. The
`examine` call itself stays where it is, because `blocked` is decided by it.

The test that the two agree is the structure rather than an assertion: they call
one function. What is asserted is the part a structure cannot promise — that the
deciding pass builds no sentence, checked by handing it a closure that panics if
anybody calls it.
