---
id: 27
title: 'Section footers: the actions each list offers'
type: feature
status: backlog
milestone: v0.2
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: nav
effort: s
---

## Problem
Creating a workspace means knowing the prefix key. The actions a list supports
should be visible in the list.

## Proposal
A dim footer row under each section, left-aligned actions and a menu on the
right, as herdr has:

    new                                                       menu
    ↵ go  ·  _ open  ·  e all

Clickable, and a statement of what the section can do. Costs one row per
section, which is what it is worth.

## Acceptance criteria
- [ ] Per-section actions, clickable and keyed
- [ ] The footer disappears with its section when collapsed
- [ ] Actions that cannot apply right now are dim rather than absent, so the
      row does not change width as state changes
