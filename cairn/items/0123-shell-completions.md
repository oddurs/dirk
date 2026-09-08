---
id: 123
title: Shell completions
type: feature
status: backlog
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: packaging
effort: s
---

## Problem
The CLI has grown past the point where you can remember whether it is
`pane read` or `pane show`, and there is nothing to press Tab against.

## Proposal
`dirk completion bash|zsh|fish` prints a completion script, generated from the
same command table as everything else. `make install` puts them where each shell
looks, and INSTALL says what a packager should do with them.

Complete the dynamic parts too — workspace and pane ids, agent names, board
names — by asking a running session, and degrade to nothing when none is
running rather than hanging.

## Acceptance criteria
- [ ] bash, zsh and fish
- [ ] Generated from the command table
- [ ] Ids, agent names and board names complete from the running session
- [ ] No running session means no completions, promptly
- [ ] `make install` installs them; INSTALL documents the paths
