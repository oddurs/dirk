---
id: 40
title: Search the scrollback
type: feature
status: backlog
milestone: v0.5
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: mux
effort: m
---

## Problem
Finding the error a build printed four minutes ago means scrolling and reading.

## Acceptance criteria
- [ ] Incremental search over the focused pane's scrollback and screen
- [ ] Matches highlighted, next and previous bound
- [ ] Search across every pane in the session, with results naming their pane
- [ ] Leaving search returns to where you were, not to the bottom
