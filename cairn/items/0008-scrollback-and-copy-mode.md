---
id: 8
title: Scrollback and copy mode
type: feature
status: done
milestone: v0.5
created: 2026-09-06
updated: 2026-09-07
priority: p0
area: mux
---

## Problem
vt100 keeps scrollback and dirk never shows it. There is no way to look at
output that has left the screen, and no way to copy anything out.

## Acceptance criteria
- [ ] Wheel and keys scroll the focused pane
- [ ] A visible indication that you are not at the bottom
- [ ] Select with the pointer, copy to the system clipboard
- [ ] Entering copy mode does not disturb the program inside
