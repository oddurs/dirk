---
id: 171
title: The things you close have something to press
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: chrome
depends_on:
- 156
- 157
- 160
---

## Problem
`✕` appears once in dirk: the quit button in the rail. Nothing else that can be
closed has anything to press.

A space closes with `x` on its last pane or through the API. A tab closes with
`&`. A pane closes with `x`. All three are things people close many times a day
and all three are invisible to a pointer.

The rail's quit button is also the proof that this works and that the pattern
is already decided: it takes two clicks, the first arms it and says so. Closing
things that hold live work is exactly that problem again.

## Proposal
A close control on each thing that closes, in the place its kind of thing has
one: the pane's strip, the tab's own cell in the tab strip, the space's row at
its right edge.

The rule that makes this safe is the rail's, applied outward. **Closing
something that holds live work asks first; closing something that does not,
does not.** A shell sitting at a prompt goes without a question. A pane running
a build, a tab with a running agent, a space with either -- those arm rather
than close, exactly like quit, and say what they are about to end.

The space's control is the one to be most careful with. A space row is small,
already carries a state mark, a number, a branch, counts and an age, and now a
disclosure at the right edge. The close goes beside the disclosure and appears
on hover rather than permanently -- which is the one case in this milestone
where hover reveals a control rather than merely acknowledging one, and it is
worth the exception because the alternative is a permanent `✕` on every row
inviting a mis-click that ends an agent.

## Acceptance criteria
- [ ] Pane, tab and space each have a close control where their kind puts one
- [ ] Closing something with live work arms first and says what it will end
- [ ] Closing something idle happens on the first press
- [ ] The space's control appears on hover; the others are permanent
- [ ] No control changes the width of the row it is on
- [ ] Every one of them has the key that does the same thing, unchanged
