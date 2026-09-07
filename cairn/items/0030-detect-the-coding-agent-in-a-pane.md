---
id: 30
title: Detect the coding agent in a pane
type: feature
status: done
milestone: v0.3
assignee: oddurs
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
- [x] Claude Code and Codex recognised, with a table that takes more
- [x] A shell at its prompt is reported as available, not as an unknown agent
- [x] Detection survives the agent being started after the pane
- [x] Cheap enough to run on the tick; cached between samples
- [x] Works on linux and macos, which have different process APIs

## Plan

The signal is the tty's **foreground process group**, which is exactly "what is
running in this pane right now" — the thing a shell sets before it waits and
resets when the command ends. `portable-pty` exposes it as
`process_group_leader()`, one `tcgetpgrp` per pane, cheap enough for the tick.

Turning a pgid into a name needs the process table. One `ps -A -o pgid=,comm=`
per tick for the whole session rather than one per pane, run off the drawing
thread and delivered as an event, for the same reason git is: a blocked `ps`
must not be able to stop the program redrawing.

`comm` is sometimes a full path and sometimes a bare name, so it is reduced to a
basename before matching. Checked on this machine: Claude Code reports as
`claude`, and the shells report as `zsh`, `bash`, `sh`.

Three answers, not two. A pane is running an **agent**, or is a **shell at its
prompt** — which is what `agent start` will need to know is available — or is
running something else entirely, which is neither and should not be guessed at.
Reporting "unknown agent" for a running build is how a status column becomes
noise.

Deliberately not parsing `args`: a title is how an agent says what it is doing
and `comm` is what it is, and reading a command line to identify a program is
guessing at someone else's argv.
