---
id: 175
title: A trackpad is not a wheel with two directions
type: chore
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: s
area: mux
depends_on:
- 158
---

## Problem
dirk understands four scroll directions and uses two. `ScrollLeft` and
`ScrollRight` are decoded in the wire and by `encode_mouse`, and no part of the
interface does anything with them.

That is the small half. The larger half is that a trackpad does not produce
what a wheel produces. A two-finger flick is a burst of events with momentum
behind it, and dirk scrolls three lines per event unconditionally -- so the
same physical gesture moves a different distance on a mouse and on a trackpad,
and a hard flick on a trackpad jumps a screenful and overshoots whatever you
were reading toward.

For a keyboard-driven application that is a small annoyance. For one where the
wheel is how reading works, it is the most-used input being wrong.

## Proposal
Three, in order of certainty.

**Use the horizontal directions.** In the nav they are nothing, and should
stay nothing. Over a pane they belong to the program, and `encode_mouse`
already forwards them correctly -- so this is mostly a matter of confirming
that and not swallowing them on the way.

**Stop multiplying blindly.** Three lines per event is right for a wheel notch
and wrong for a burst. The events that arrive close together are one gesture
and should move a distance related to the gesture, not to how many packets it
was chopped into.

**Do not invent momentum.** Terminals do not report pressure or velocity and
guessing at it from arrival times is how scrolling becomes unpredictable in a
way nobody can describe. Coalescing a burst is defensible; simulating physics
from timestamps is not, and if the honest answer is that a terminal cannot tell
a trackpad from a wheel then that gets written down and the multiplier becomes
configurable instead.

## Acceptance criteria
- [ ] Horizontal scroll reaches a program that asked for it and is otherwise inert
- [ ] A burst of wheel events scrolls a sensible distance, not three lines times the burst
- [ ] The behaviour is described in one place, with the reasoning
- [ ] No velocity or momentum is inferred from event timing
- [ ] The distance is configurable if the conclusion is that dirk cannot tell
