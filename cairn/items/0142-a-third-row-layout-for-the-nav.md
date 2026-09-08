---
id: 142
title: A third row layout for the nav
type: feature
status: done
milestone: v0.8
created: 2026-09-07
updated: 2026-09-08
priority: p3
area: nav
effort: s
---

## Problem
`rows = "tall" | "short"` is two arrangements, and the gap between them is
large: tall gives a workspace a second line for its branch, short drops it
entirely. There is no arrangement that keeps the branch and loses the age, or
keeps the age on one line.

## Proposal
A third named layout, not a composition mechanism. herdr lets you assemble rows
from token arrays, which is more power than this needs and invites arrangements
whose columns do not line up — and the nav's column arithmetic being exact is
what keeps it readable.

Pick the third arrangement worth having, name it, and document all three
together so the choice is a choice rather than a search.

## Acceptance criteria
- [x] A third value for `rows`, named for what it shows
- [x] Column arithmetic exact in all three, at every sidebar width
- [x] All three documented in one table
- [x] No token composition mechanism

## 2026-09-08

The item described the two that existed before the nav redesign: it says `tall`
gives a space its branch on a second line. It is the other way round now — the
row *leads* with what the space is, which is its branch, and the second line is
the caption saying what it is doing. `0092` made that change and this item
predates it.

So the third one worth having is the mirror of `short`: one line carrying what
the space is *doing* rather than what it *is*. That is the arrangement for
somebody whose projects are one checkout each, where every row says `main` and
the branch is the column that carries nothing.

One filter and no new drawing path: the row already chooses the label when there
is no branch, so `intent` is that same row with the branch withheld. The column
arithmetic is therefore the arithmetic that was already exact.
