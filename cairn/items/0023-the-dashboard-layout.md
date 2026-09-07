---
id: 23
title: The dashboard layout
type: feature
status: done
milestone: v0.2
depends_on:
- 22
- 24
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: layout
effort: m
---

## Problem
The layout mechanism needs its first real user, and the author already has the
one he wants: brief, ptop, spaces, attention and cairn in five panes.

## Proposal
Ship it as a default layout when its programs are present, so a fresh install
that already has ptop and cairn gets the dashboard without configuring anything.

Depends on the panel spike (see the spike on where the panels come from).

## Acceptance criteria
- [x] Five panes: brief across the top, ptop and spaces beside each other,
      attention and cairn below
- [x] Present by default when its programs are on PATH, absent when they are not
- [x] Documented in the manual as the worked example of a layout
