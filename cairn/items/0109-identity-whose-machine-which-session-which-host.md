---
id: 109
title: 'Identity: whose machine, which session, which host'
type: feature
status: backlog
milestone: v0.5
labels:
- rail
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: chrome
---

## Problem

Column one of the rail — first in the reading order, the strongest position the
bar has — holds `◆ dirk`. It never changes, it cannot be clicked, and it
answers "what program is this", which you knew before you started it.

The file says the slot's job is *where am I*. A product name cannot answer that.

What is genuinely unanswered is: **whose account, which session, and which
machine.** `dirk --remote prod` and a local dirk are identical on screen. Two
terminals side by side, one on a laptop and one on a production host, and
nothing in the interface distinguishes them. That is not a missing nicety. It is
how someone runs the right command in the wrong place.

## Decision

**The name defaults to `$USER`. The mark keeps the brand.**

The mark does the branding job completely in one cell — nothing else draws `◆`
in the bottom-left corner of a terminal — and it does it without spending the
slot that should be answering a question. Anyone who wants the word back sets
one line of configuration, which is already possible today.

## Proposal

    ◆ oddurs                    local, default session
    ◆ oddurs · api              a session that was named
    ◆ oddurs@prod · api         a session on another machine

The host is coloured rather than dim, because being attached to another machine
is a state and not a label. Under pressure it is the last part of the identity
to be dropped — after the session name, after the context, before only the
user's own name.

    [identity]
    name    = ""         # empty means $USER
    mark    = ""         # empty means the glyph set's ◆
    session = "named"    # never | named | always
    host    = "remote"   # never | remote | always

The defaults are chosen so the ordinary case stays quiet — five cells more than
today, one more question answered — and the dangerous case is loud without
having to be configured into existence first.

`[brand]` becomes `[identity]`, with `[brand]` still read and honoured so no
existing configuration file breaks.

## Acceptance criteria

- [ ] With no configuration, the rail shows the mark and `$USER`
- [ ] A session started with `--session NAME` shows that name
- [ ] A session reached with `--remote HOST` shows the host, in its own colour
- [ ] An existing `[brand] name = "dirk"` still puts `dirk` in the rail
- [ ] The smoke test asserts the mark is in the rail, rather than the word

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0
