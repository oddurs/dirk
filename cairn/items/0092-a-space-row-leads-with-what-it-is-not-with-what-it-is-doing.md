---
id: 92
title: A space row leads with what it is, not with what it is doing
type: feature
status: review
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: nav
effort: m
---

## Problem
A space draws its intent on the first line and its branch on the second:

    * 1 Building the mux core     2m
        main

The intent is the thing that changes.  It is rewritten every time the agent
revises what it says it is doing, so the line you scan for a place to go is the
line that will not hold still -- and the identity, which does hold still, is
underneath in the colour of scenery.

## Inverted
Identity first, intent second:

    ◐ 4 · try ↑6 ↓1                    ▾
      Namesync multiplexer TUI

The number and the branch are stable, so the column stops moving under you.
The intent is still there, one line down, where it reads as a caption of the
thing above it -- which is what it is.

## Acceptance criteria
- [ ] The first line is state, number, branch and git counts
      — state, number and branch done here; the counts are 0093
- [x] The second line is the intent
- [x] A space with nothing to say for its second line draws one line
- [x] The short form keeps the identity line and drops the intent
