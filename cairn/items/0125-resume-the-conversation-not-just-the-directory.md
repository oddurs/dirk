---
id: 125
title: Resume the conversation, not just the directory
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: agents
effort: l
---

## Problem
A session restored after the server stops brings back workspaces, panes, split
tree, working directories and focus — and a bare shell in each pane. The agent
that was three hours into a refactor is gone, and its conversation is sitting on
disk under an id nobody wrote down.

Of everything in this milestone, this is the one that changes how the tool
feels. Restoring the shape of the work while losing the work is close to the
worst possible place to stop.

## Proposal
Every agent harness that can resume can name its session: `claude --resume <id>`,
`codex resume <id>`, `opencode --session <id>`. The hook that already reports
state reports that reference too, dirk records it beside the pane in the session
file, and restore relaunches the agent with it instead of starting a shell.

The reference is a claim about a moment like any other. A stale, duplicated or
unreadable one restores as a normal shell in the saved directory rather than
failing the restore — a pane that comes back wrong is worse than a pane that
comes back empty.

Resume happens once a client has attached and given a terminal size, and across
every workspace rather than only the focused one, so the work is running by the
time you get to it.

## Acceptance criteria
- [ ] The hook snippet reports a session reference alongside state
- [ ] The reference is persisted with the pane and survives a restart
- [ ] Restore relaunches with the harness's own resume argument
- [ ] The resume argument is per-harness configuration, not a match arm
- [ ] A missing, stale or unreadable reference restores a plain shell
- [ ] `[session] resume_agents = false` turns it off
- [ ] Resume waits for a client's terminal size, then runs for every workspace
