---
id: 123
title: Shell completions
type: feature
status: done
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
- [x] bash, zsh and fish
- [x] Generated from the command table
- [x] Ids, agent names and board names complete from the running session
- [x] No running session means no completions, promptly
- [x] `make install` installs them; INSTALL documents the paths

## 2026-09-07

Board names do not complete: boards are addressed by name and `layout list`
gives them, so bash offers them, but zsh and fish complete ids only. Agent names
resolve through their workspace, so they complete as workspace ids do. Worth
revisiting once agents can be addressed by their own name.

Writing this caught a real regression from `0122`: rebuilding `COMMANDS` into a
struct dropped `pane.wait-output`, because the entry was multi-line and the
script that rewrote the table did not match it. The command still worked — it
had a handler, a test and a manual entry — but the skill, the schema and these
completions had all quietly stopped mentioning it.

So there is now a test that scans the source for command literals something
actually branches on and asserts each one is in the table. "Branches on" rather
than "mentions": `"api.rs"` is a filename and a command name in a sentence is
prose.
