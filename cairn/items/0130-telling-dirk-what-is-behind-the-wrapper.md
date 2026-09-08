---
id: 130
title: Telling dirk what is behind the wrapper
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: agents
effort: s
---

## Problem
Detection starts from the foreground process. An agent run under a sandbox, a
container shim or any other wrapper shows the wrapper as the foreground process,
so dirk sees `nono` or `fence` and no agent at all — no state, no naming, no
notification, in exactly the setup where an agent is most likely to be left
running unattended.

## Proposal
`DIRK_AGENT=claude` on the wrapper's command line names which shipped ruleset to
use for that foreground process. Read from the process environment, applied to
that process only, and not inherited — an exported value would make every shell
you open claim to be an agent.

Ten lines, and it unblocks everyone running an agent under a jail.

## Acceptance criteria
- [x] A wrapper's command line selects that ruleset — see the note below
- [x] It applies to that process only, and is not inherited by children
- [x] A name with no rules behind it is ignored — see the note below
- [x] `agent explain` reports that the agent was found behind a wrapper
- [x] Documented, with the closed-list rule that replaces the caveat

## 2026-09-08

Not `DIRK_AGENT`. The environment variable requires reading another process's
environment, and macOS does not allow it: `ps -E -o command= -p PID` prints the
command and no environment, and `KERN_PROCARGS2` comes back without it too.
Shipping a detection feature that works on one of two supported platforms is
worse than shipping a different one that works on both.

The mechanism that does work was already here. dirk reads a command line to
disambiguate a program that has told it nothing, for interpreters and nothing
else, and a sandbox is the same shape: `fence -- claude` is `node claude/cli.js`
with a different first word. So the closed list became configuration —
`wrappers = ["fence", "nono"]` — and the `argv` fragments that already identify
an agent under `node` identify one under a sandbox unchanged.

That also disposes of two criteria rather than meeting them. There is no name to
be unknown, because the name is not asserted by the wrapper — it is matched
against rules dirk holds. And there is no global-export footgun, because nothing
is read from an environment: a program nobody listed keeps its own identity even
when an agent's name is in its arguments, which is what the second test pins.
