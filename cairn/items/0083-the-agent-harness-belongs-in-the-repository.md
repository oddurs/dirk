---
id: 83
title: The agent harness belongs in the repository
type: chore
status: doing
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: docs
---

## Problem

dirk is a multiplexer for watching coding agents work, and it is itself written
mostly by them. The instructions they need — the loop, the traps, the style the
code is held to — live in one person's memory and in whatever was said in the
last session. AGENTS.md carries cairn's block and nothing else.

There is also an attribution problem. Harnesses add `Co-authored-by` trailers
and generated-with footers to commits by default. This project's history should
say who decided things, not which tool typed them, and once such a line is in a
published history it cannot be taken out without a rewrite.

## Proposal

Everything an agent needs, checked in beside the code:

- `AGENTS.md` grows the project's own sections above cairn's generated block:
  the loop, the gate, what the code is trying to be, and the traps that have
  already caught someone.
- `CLAUDE.md` imports it, so there is one source and not two that drift.
- `.claude/agents/` holds subagents for the jobs this repository actually has:
  the backlog, pty smoke tests, the chrome, NEWS, review, and the docs.
- `.claude/commands/` holds the loop as slash commands, so `/ship` is the same
  thing `git work ship` is.
- `.claude/settings.json` turns attribution off.
- `.githooks/commit-msg` strips it anyway, for every harness and every agent,
  because a setting only covers the harness that reads it.

## Acceptance criteria

- [ ] AGENTS.md tells an agent enough to work without being told anything else
- [ ] Subagents exist for the backlog, tests, chrome, NEWS, review and docs
- [ ] No commit can carry a tool's name as an author or a trailer
- [ ] `make setup` installs the hook that enforces it
