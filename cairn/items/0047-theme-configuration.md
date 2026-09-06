---
id: 47
title: Theme configuration
type: feature
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: config
effort: m
---

## Problem
The Gotham palette is compiled in. It is a good palette and it is not everyone's,
and the tokens are already semantic — the file is one step from being data.

## Acceptance criteria
- [ ] A theme in config.toml, addressing the same semantic tokens
- [ ] The built-in palette is the default and needs no configuration
- [ ] A missing token falls back rather than failing to load
- [ ] Enough contrast checking to refuse an unreadable combination
