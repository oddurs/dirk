---
id: 51
title: An agent run under an interpreter reports as the interpreter
type: bug
status: done
milestone: v0.3
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: agents
effort: m
---

## What happens
Detection reads the foreground process group's `comm`, which is the executable.
An agent installed through npm is executed as `node`, so `comm` reads `node`
and the pane is classified as an ordinary program: no state glyph, absent from
the agents list, and reported free for `agent start` while an agent is running
in it.

Found in review of 0030. Not reachable on the machine this was built on, where
Claude Code is a native binary and `comm` reads `claude` — which is exactly
why it is worth writing down rather than trusting.

Reading `/proc/<pid>/exe` does not help: that is `/usr/bin/node` too. The only
place the agent's identity appears is the command line.

## What should happen
An agent is recognised however it was installed.

## Proposal
A narrow exception rather than general argv parsing. When `comm` is a known
interpreter — node, python, deno, bun, ruby — and only then, look at the command
line for a known agent's entry point. The kind table gains an optional second
matcher used only in that case.

The rule this preserves: dirk does not read someone else's argv to work out what
they are. It reads it to disambiguate an interpreter that has told us nothing.

## Acceptance criteria
- [x] An npm-installed agent under `node` is recognised
- [x] The interpreter list is closed, and argv is not consulted for anything else
- [x] Linux truncates `comm` to 15 characters; the table accounts for it
- [x] A test with a script named like an agent, run under an interpreter
