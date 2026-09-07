---
id: 21
title: The agents section, ordered by attention
type: feature
status: done
milestone: v0.2
assignee: oddurs
depends_on:
- 31
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: nav
effort: m
---

## Problem
Spaces are listed in a stable order so that the number beside one is a jump key
you can learn. That is the right order for navigation and exactly the wrong
order for triage: the agent that has been blocked for ten minutes is wherever
its workspace happens to sit.

## Proposal
A second list of the same workspaces, ordered by what is owed:

    blocked   waiting on you, oldest first
    done      finished work you have not seen
    working   in flight
    idle      nothing owed, most recent first

Each row keeps its number, so the jump key does not change when the order does.
Age replaces the branch on the second line, because in this list the question is
how long something has been waiting.

Depends on real agent states (0009) to be more than a re-sort of a guess; ships
before them so the shape is settled first.

## Acceptance criteria
- [x] Ordered by attention, with ties broken by age
- [x] Numbers stay attached to the workspace, not the position
- [x] The section is empty and takes no space when nothing is owed
- [x] Clicking a row focuses that workspace and marks it seen
