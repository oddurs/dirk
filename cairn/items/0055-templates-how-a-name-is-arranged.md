---
id: 55
title: 'Templates: how a name is arranged'
type: feature
status: done
milestone: v0.3.1
assignee: oddurs
depends_on:
- 54
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: naming
effort: m
---

## Problem
The label is `{intent}` with no way to say otherwise. namesync has a template
per target — workspace, tab, agent — so a label can carry the branch, or the
project, or the age, in whatever order suits the person reading it.

## Acceptance criteria
- [x] `{token}` substitution over the tokens from 0054
- [x] A template per target: workspace and agent (tabs do not exist yet)
- [x] An unknown token renders as nothing rather than as its own name
- [x] Whitespace collapses, so absent tokens do not leave gaps
- [x] The default is `{intent}`, which is what it does today
