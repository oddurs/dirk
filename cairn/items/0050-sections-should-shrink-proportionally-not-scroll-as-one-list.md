---
id: 50
title: Sections should shrink proportionally, not scroll as one list
type: feature
status: backlog
milestone: v0.2
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: nav
effort: m
---

## Problem
The nav is one flat list of rows with one scroll offset, so a long spaces list
pushes the agents section off the bottom entirely rather than each section
giving up some height. The section that gets cut is whichever is last, which is
agents — the one that exists to be noticed.

This was an acceptance criterion of 0026 and was ticked there by mistake; found
in review. Recorded here rather than quietly left, because a ticked box that
was never built is worse than an open one.

## Proposal
Allocate height per section before rendering: each gets what it needs up to a
share of what is available, and only sections that are over their share scroll.
A section with three rows should never be scrolled at all.

## Acceptance criteria
- [ ] Each section is allocated height before any of them is drawn
- [ ] A section that fits never scrolls
- [ ] A section that does not scroll within itself, keeping its heading visible
- [ ] The agents section is never the one silently cut off
- [ ] Selection moving into a section scrolls that section, not the whole nav
