---
id: 167
title: When there is no pointer
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: s
area: config
depends_on:
- 155
- 161
---

## Problem
Mouse-first has an obvious failure: the machine with no pointer. A link that
will not report motion, a terminal that does not speak SGR, a session inside
another multiplexer that is eating the events, `mouse = false` set on purpose,
and ssh from a phone.

If hover is the only thing that says a row is clickable, and the context menu
is the only thing that lists what applies to it, then on those machines dirk
becomes an interface with no visible affordances *and* no discoverable actions
-- worse than it is today, because today the footers at least print the keys.

## Proposal
Three things, in order of how much they matter.

**Notice.** dirk can tell whether mouse reporting was accepted, and should stop
assuming. When there is no pointer, say so once rather than behaving as though
one is about to appear.

**Degrade to the keyboard, deliberately.** With no pointer, the footers stay
printing keys rather than becoming buttons, and the context menu is reachable
from the selected row by key -- which the right-click item already requires for
its own reasons, and which turns out to be the whole of the fallback.

**Do not degrade in the middle.** `pane borders, and turning the mouse off`
already argues this for `mouse = false`: all or nothing, because a
half-captured mouse is a mode you have to remember. The same holds here. A
terminal that reports clicks but not motion gets clicks and no hover, and that
is a stated combination rather than an accident.

The keyboard-complete half of the principle is what makes this cheap. If that
test passes, this item is about telling the truth on screen rather than about
building a second interface.

## Acceptance criteria
- [ ] dirk notices when there is no mouse rather than assuming there is
- [ ] With no pointer, every action is still reachable and the chrome still says how
- [ ] The context menu opens from the keyboard on the selected row
- [ ] Clicks without motion is a supported combination, not a broken one
- [ ] `mouse = false` and a terminal that refuses reporting behave the same way
- [ ] A test runs the suite with the mouse off and everything still passes
