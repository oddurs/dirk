---
id: 162
title: What can be clicked looks different from what cannot
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: chrome
depends_on:
- 160
---

## Problem
Things that can be clicked look exactly like things that cannot. A space row
and a section heading are both text on the same ground; the two ways out in the
corner are the words `detach` and `✕ quit`; the footers under each section --
`n new · o project`, `↵ go · s sort` -- are a key and a word, which is a
sentence about the keyboard printed where a mouse-first interface would put its
buttons.

Those footers are the sharpest case. They are already clickable. Nothing about
them says so, and everything about them says "press n".

## Proposal
An affordance pass over the chrome, once hover exists to make it verifiable.

The footers become buttons that also name their key, rather than key hints that
happen to accept a click. The two ways out get an edge that says they are
pressable, which matters more for them than for anything else on screen --
`quit` ends every shell and agent, and it currently looks like a caption.

The rule to hold to, so this does not become decoration: **an affordance says
what will happen, not that something is interactive.** A row that changes when
pointed at has said it. A button drawn in a box that does nothing on hover has
not.

Nothing here changes a width. The nav's arithmetic is exact and this pass is
about weight, ground and edges, not about inserting characters into rows that
are already counted to the column.

## Acceptance criteria
- [ ] Clickable chrome is distinguishable from text before it is pointed at
- [ ] Footers read as buttons and still name their keys
- [ ] Detach and quit look pressable, and look like different weights of thing
- [ ] No affordance changes the width of anything
- [ ] The ascii glyph set gets the same treatment, not a degraded one
