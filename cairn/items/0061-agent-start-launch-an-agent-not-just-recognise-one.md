---
id: 61
title: 'agent start: launch an agent, not just recognise one'
type: feature
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: agents
effort: m
---

## Problem
`dirk agent list` reports what is running; nothing starts one. An agent
delegating work has to `pane split` and then type a command into the new pane,
which means knowing how each agent is invoked and having no way to tell whether
it came up.

## Why it was not in 0035
Recognising an agent and launching one are different problems. The kind table
knows what a running agent *looks like*; it does not know the command, the flags
or how to tell that the thing has finished starting. herdr's `agent start`
returns only once it has seen the agent it expected and considers it ready — and
returns `agent_not_ready` if it comes up blocked.

## Acceptance criteria
- [ ] `dirk agent start <name> --kind <kind> --pane <id>` in an available pane
- [ ] Refuses a pane that is not at a prompt, rather than typing over something
- [ ] Returns once the agent is detected and ready, or says it is blocked
- [ ] Native arguments passed through after `--`
- [ ] The name is taken at start and is unique among live agents
