---
id: 38
title: Reload configuration without restarting
type: feature
status: backlog
milestone: v0.4
depends_on:
- 7
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: config
effort: s
---

## Problem
Changing a layout or a colour means killing every pane in the session.

## Acceptance criteria
- [ ] A command re-reads config.toml and applies what can be applied live
- [ ] Layouts, theme, naming thresholds and pages take effect immediately
- [ ] A bad file is reported and the running configuration is kept
- [ ] What cannot be changed live says so rather than silently not applying
