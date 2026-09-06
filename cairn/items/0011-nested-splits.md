---
id: 11
title: Nested splits
type: feature
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: mux
---

## Problem
A workspace splits into equal columns or equal rows and no further. That was a
deliberate v1 simplification — the moment a workspace needs a nested layout it
probably wanted to be two workspaces — but the ceiling is real.

## Acceptance criteria
- [ ] A layout tree rather than a flat vec
- [ ] Drag a border to resize
- [ ] Zoom one pane to fill the workspace and back
