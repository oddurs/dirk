---
id: 116
title: 'agent prompt: give an agent work over the socket'
type: feature
status: backlog
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
- [ ] `agent prompt <target> <text>` submits text and Enter as one ordered write
- [ ] Bracketed paste is used when the pane has it on
- [ ] An agent already `blocked` is refused, with an error naming that reason
- [ ] `--wait` observes activity before waiting for the settled state
- [ ] `--timeout` is honoured, and the timeout error says input may have been sent
- [ ] The answer carries the agent's state at the moment the command returned
