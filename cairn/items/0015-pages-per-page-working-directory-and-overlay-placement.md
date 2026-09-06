---
id: 15
title: 'Pages: per-page working directory and overlay placement'
type: feature
status: backlog
milestone: v0.1
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: ui
---

## Problem
Pages all spawn in $HOME, so `lazygit` opens on nothing useful and `cairn board`
shows no project. smali solved this with overlay panes that restore the previous
focus on exit; dirk currently swaps the whole content area.

## Acceptance criteria
- [ ] A page can run in the focused workspace's directory
- [ ] Opening a page and quitting it returns you exactly where you were
- [ ] A page whose program is missing is dropped at load, as it already is
