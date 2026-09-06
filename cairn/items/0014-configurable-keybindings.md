---
id: 14
title: Configurable keybindings
type: feature
status: backlog
milestone: v0.5
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: config
---

## Problem
The prefix is Ctrl-Space and every command key is a literal in a match arm.
Ctrl-Space is a defensible default — Ctrl-a and Ctrl-b are both load-bearing in
a shell line editor — but it should not be the only option.

## Acceptance criteria
- [ ] Prefix and commands bound from config.toml
- [ ] A visible list of the current bindings
- [ ] An unknown binding is reported at load rather than silently ignored
