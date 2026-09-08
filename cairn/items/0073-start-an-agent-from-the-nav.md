---
id: 73
title: Start an agent from the nav
type: feature
status: done
milestone: v0.5
depends_on:
- 61
- 67
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: agents
effort: m
---

## Problem
`n` makes a workspace with a shell in it, and starting an agent is something you
then type.  But the shell is almost never what was wanted: the workspace exists
in order to hold an agent, which is why the naming policy is built entirely
around what the agent says it is doing.  A workspace with a shell in it has no
intent, gets no name, and sits in the tree called after its project.

## The shortcut has to answer the question for you
Which agent comes from the project, then the global default, then a picker.  A
Rust repo and a TypeScript repo can want different tools, and answering that
question twelve times a day is how a shortcut stops being one.

```
   + agent   + workspace
   n new  ·  a agent  ·  o open
```

Agent first in the footer, because it is the common case.

## Shares a primitive with 0061
0061 is the same launch seen from the socket: an agent delegating to another
agent, which needs a name, a readiness answer, and a refusal when the pane is
busy.  This is the same launch seen from the keyboard.  One implementation with
two callers -- building the nav path separately is how they drift, and the
refusal rule in particular is not something to write twice.

## Shape
* `a` on a workspace row starts an agent in the focused pane, if that pane is
  available.  On a project row it means new workspace, then agent in it, which
  is the thing you actually do.
* Refuses a pane that is not at a prompt rather than typing over something --
  0061's rule, and the reason `available` exists in the API.
* The picker is `src/ui/picker.rs`, already built for this shape.
* `[[project]] agent = "claude"` names a project's default; `[agents] default`
  names the global one; neither means the picker, not an error.
* The command for a kind comes from the `[[agent]]` block in 0067, which is why
  this waits for it.

## Acceptance criteria
- [ ] `a` starts an agent; on a project row, a new workspace and an agent in it
- [ ] The footer offers `+ agent` before `+ workspace`
- [ ] A busy pane is refused, not typed into
- [ ] `[[project]] agent` then `[agents] default` then a picker
- [ ] It reports when the agent is up, or that it came up blocked
- [ ] The launch path is shared with `agent start`, not duplicated
