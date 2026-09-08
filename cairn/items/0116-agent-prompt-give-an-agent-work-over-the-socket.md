---
id: 116
title: 'agent prompt: give an agent work over the socket'
type: feature
status: done
milestone: v0.6
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: api
effort: m
---

## Problem
`dirk agent start` puts an agent in a pane and `agent list` says what state it
is in. Between those two there is nothing: the only way to give an agent work is
to type at it. So dirk can be asked about agents and cannot be used to run them,
which is the wrong way round for a tool whose whole claim is that it knows what
its sessions are for.

## Proposal
`dirk agent prompt <target> <text>` writes the text and a submitting newline to
the agent's terminal, honouring the pane's live bracketed-paste mode so a
multi-line prompt arrives as one paste rather than as several submissions.

`--wait` holds until the agent settles, and is where the care is. An agent that
is already blocked is refused without sending anything — the caller wants to see
the dialog, not answer it by accident. After submitting to an agent that was not
working, dirk waits a few seconds to observe `working` or `blocked` before it
starts waiting for the settled state, so an unrelated transition cannot satisfy
a wait for work that never started.

A timeout does not prove nothing was sent. Say so in the error, because the
caller's next move is to read the pane rather than prompt again.

## Acceptance criteria
- [x] `agent prompt <target> <text>` submits text and Enter as one ordered write
- [x] Bracketed paste is used when the pane has it on
- [x] An agent already `blocked` is refused, with an error naming that reason
- [x] `--wait` observes activity before waiting for the settled state
- [x] `--timeout` is honoured, and the timeout error says input may have been sent
- [x] The answer carries the agent's state at the moment the command returned

## 2026-09-07

The two-phase wait is the part worth keeping. A prompt sent to an idle agent
leaves it idle for an instant afterwards, for the same reason as before, so a
wait for `idle` would be satisfied by a state that has nothing to do with the
prompt. It therefore watches for the agent to be *working* or *blocked* first,
and only then waits for it to settle.

Five seconds for that: long enough that a harness slow to redraw is not accused
of ignoring the prompt, short enough that a caller waiting on one that went
nowhere is not left there. A very fast turn that completes inside the sampling
interval could in principle be read as stalled; the alternative — satisfying the
wait from the state before the prompt — is wrong far more often.

`agent.prompt` acts before it decides whether to wait, which the answer path did
not have a shape for: `hold` returned "this is a wait" or "this is ordinary",
and ordinary meant running the command again. Hence `Begin`, with a third arm
for a command that has already done its work and only needs its answer sent.

A timeout on a prompt says the prompt was sent. It is the one timeout that
proves something happened, and a caller retrying on the strength of it would
submit the same work twice.
