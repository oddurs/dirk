---
id: 52
title: Naming should hold off while an agent is blocked
type: feature
status: done
milestone: v0.3
assignee: oddurs
depends_on:
- 31
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: naming
effort: s
---

## Problem
A blocked agent's terminal title describes the question it is asking, not the
work it is doing. Naming a workspace from it produces a label like "Do you want
to proceed", which is both wrong and sticky — it is what the workspace is called
until the next intent arrives.

The ported policy has always had the branch for this. It could not reach it,
because dirk had no `blocked` state to pass in. 0031 gives it one; the two are
not yet connected.

## Proposal
Pass the workspace's state into `name::decide`, and take the branch that is
already written. herdr's policy also retries shortly afterwards rather than
waiting for the next event, since unblocking produces no title change of its own.

## Acceptance criteria
- [x] `decide` is given the agent state
- [x] A blocked workspace is not renamed
- [x] The name that was there before the block is unchanged by it
- [x] Naming resumes once the agent is unblocked, without needing a new title
- [x] A test that a blocked agent's question never becomes a workspace name
