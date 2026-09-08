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
- [ ] `DIRK_AGENT` on a pane's foreground process selects that ruleset
- [ ] It applies to that process only, and is not inherited by children
- [ ] An unknown name warns once and is otherwise ignored
- [ ] `agent explain` reports that the hint decided the harness
- [ ] Documented with the caveat about exporting it globally
