---
id: 19
title: 'Sidebar: three sections — layouts, spaces, agents'
type: feature
status: planned
milestone: v0.2
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: nav
effort: l
---

## Problem
The sidebar has two sections, pages and projects, and neither is what the nav
should be. `pages` is a list of single programs; the thing above spaces should
be a list of *layouts*, which are named multi-pane arrangements. And there is no
agents section at all, so the question the nav exists to answer — which agent
needs me — is not asked anywhere.

## Proposal
Three sections, top to bottom, each with a letter-spaced heading and a count on
the right:

    L A Y O U T S                    3
      Overview
      Review
      Focus
    S P A C E S                     10
      ...
    A G E N T S            priority
      ...

They are three lists of different things and the ordering is deliberate:
layouts are places you go, spaces are where work lives, agents are what is
asking for you. Attention flows down the column.

The same workspace appears in both spaces and agents. That is not duplication:
spaces answers 'what is open, and where', agents answers 'what needs me', and
they sort differently for that reason.

## Acceptance criteria
- [ ] Three sections, each independently collapsible
- [ ] A count per section header
- [ ] Sections keep their proportions when the terminal is short, rather than
      the last one being cut off entirely
- [ ] Every row in all three is a click target
