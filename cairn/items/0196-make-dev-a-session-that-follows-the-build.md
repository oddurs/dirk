---
id: 196
title: 'make dev: a session that follows the build'
type: chore
status: done
milestone: v0.10
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: s
area: packaging
---

## Problem

Seeing a change to dirk running meant `make install` and a restart, which
ends every shell and agent in the session -- so the running session was the
release binary and the change was only ever seen in a test.

## Proposal

`make dev`: build this checkout, keep the binary at a stable path under
`~/.cache/dirk/dev`, and on every change to `src`, `tests` or the manifest
rebuild and `session handoff` the running `dev` session to it. `dirk-dev`
attaches, from any terminal. A worktree's build reaches a session started from
another checkout, because the path the server execs is the stable one.

Needs `[session] handoff = true` in the configuration, and the script does not
set it: the manual says why it is off.

## Acceptance criteria

- [x] `make dev` builds, watches, and hands off on change
- [x] A shell in the dev session survives the handoff; the attached terminal
      comes back on its own (item 0195)
- [x] `shellcheck` is clean, and the script is in the gate's list
