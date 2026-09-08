---
id: 38
title: Reload configuration without restarting
type: feature
status: done
milestone: v0.4
assignee: oddurs
depends_on:
- 7
created: 2026-09-06
updated: 2026-09-07
priority: p2
area: config
effort: s
---

## Problem
Changing a layout or a colour means killing every pane in the session.

## Acceptance criteria
- [x] A command re-reads config.toml and applies what can be applied live
- [x] Layouts, theme, naming thresholds and pages take effect immediately
- [x] A bad file is reported and the running configuration is kept
- [x] What cannot be changed live says so rather than silently not applying
