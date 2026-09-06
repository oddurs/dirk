---
id: 30
title: Detect the coding agent in a pane
type: feature
status: backlog
milestone: v0.3
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: agents
effort: l
---

## Problem
dirk cannot tell a shell from Claude Code from a build. Everything downstream —
states, attention, notifications, naming while blocked — needs to know an agent
is there and which one.

## Proposal
Walk the pane's process tree from the pty's foreground process group and match
against a table of known agent kinds. Process inspection, not title parsing:
titles are how an agent says what it is doing, not what it is.

## Acceptance criteria
- [ ] Claude Code and Codex recognised, with a table that takes more
- [ ] A shell at its prompt is reported as available, not as an unknown agent
- [ ] Detection survives the agent being started after the pane
- [ ] Cheap enough to run on the tick; cached between samples
- [ ] Works on linux and macos, which have different process APIs
