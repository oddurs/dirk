---
id: 125
title: Resume the conversation, not just the directory
type: feature
status: done
milestone: v0.7
created: 2026-09-07
updated: 2026-09-08
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
- [x] The hook snippet reports a session reference alongside state
- [x] The reference is persisted with the pane and survives a restart
- [x] Restore relaunches with the harness's own resume argument
- [x] The resume argument is per-harness configuration, not a match arm
- [x] A missing, stale or unreadable reference restores a plain shell
- [x] `[session] resume_agents = false` turns it off
- [x] Resume waits for a client's terminal size, then runs for every workspace

## 2026-09-08

The snippet does not extract the id. `--hook` says the harness has piped its
payload in and dirk reads what it understands out of it — because pulling a JSON
field in the snippet means depending on `jq`, and a hook that fails on a machine
without it stops reporting *state* as well. So the cost of a missing tool would
have been the whole feature, not this half of it.

The reference is kept against the harness that issued it, as a pair. A reference
means nothing to a program that did not mint it, and resuming codex on claude's
id would start a conversation nobody had.

Resume runs on the first attach rather than at restore. Twelve agents resumed on
a server nobody may ever look at is twelve model sessions nobody asked for, and
attaching is also the first moment there is a terminal size that belongs to a
screen rather than to a fallback.

Kept at the workspace rather than the pane. dirk's model is that a workspace
exists in order to hold an agent — it is why naming is built around what the
agent says it is doing — so that is where the conversation belongs too.

`differs` had to learn about the new fields or a conversation reported after the
last write would never reach the file, and the restart it exists for would find
nothing.

CI caught one this machine could not: the stand-in harness used `exec -a`, which
is a bashism. Linux CI's `/bin/sh` is dash, where it fails — and because the
test had `exec`ed the pane's own shell, failing took the pane, the session and
the rest of the test with it. A symlink that gives `sleep` the harness's name
does the same job with no shell feature at all.
