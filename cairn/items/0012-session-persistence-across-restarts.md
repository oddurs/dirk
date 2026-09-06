---
id: 12
title: Session persistence across restarts
type: feature
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: server
---

## Problem
Even with a daemon, a machine restart loses the tree. What is worth restoring is
the shape — which projects, which workspaces, their names — not the scrollback.

## Acceptance criteria
- [ ] The tree is written out on change and restored at startup
- [ ] Restored workspaces keep the names naming gave them
- [ ] A project that has moved or gone is dropped rather than failing the load
