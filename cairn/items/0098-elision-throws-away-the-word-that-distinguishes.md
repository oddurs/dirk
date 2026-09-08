---
id: 98
title: Elision throws away the word that distinguishes
type: bug
status: done
milestone: v0.5
assignee: oddurs
labels:
- rail
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: s
area: naming
---

## What happens

`elide` cuts the tail. Names come from intent, and intent puts the
distinguishing word last, so a column of them collapses into a column of the
same word:

    Building the mux core   ->  Building the …
    Building the release    ->  Building the …
    Reading the vt100 grid  ->  Reading the v…

The rail shows it worst, at fourteen columns, but the nav elides too and has the
same problem in a narrow sidebar.

## What should happen

When a name has to be shortened, what survives should be what tells it apart
from its neighbours. For a name of the shape *verb the noun*, that is the end.

    Building the mux core   ->  …the mux core
    Building the release    ->  …the release

This is a policy question and `src/name.rs` is where naming policy lives and is
unit tested. It is pure, so it can be decided there and tested directly rather
than through a pseudo-terminal.

Worth settling while doing it: whether shortening should prefer to drop whole
words rather than cut mid-word, and whether it should be told the sibling names
so it can keep the part that actually differs. The second is more work and is
the version that is actually correct; the first is most of the value.

## Reproduction

1. Open two workspaces whose intents start with the same word.
2. Narrow the terminal until the rail elides them.
3. Both chips read the same.

Design note: https://claude.ai/code/artifact/0540f4dc-aa3a-4eea-af94-42d910635ce0

## 2026-09-08

**Settled: whole words from the front, and no sibling awareness.**

Dropping leading words is most of the value and all of the reported case:
`Building the mux core` and `Building the release` are distinct the moment the
verb goes. Sibling awareness only earns anything when two labels agree to the
last word, and it would cost every call site the whole set of its neighbours.

Never part of a word, either. A label that begins mid-word reads as damage
rather than as shortening, and the ellipsis has already said something is
missing. The exception is one word that does not fit on its own: there is no
shared verb to drop, and a single word is identified by how it starts —
`reorganis…` is a word you can guess at and `…anisation` is not.

`shorten` is in `name.rs` beside the rest of the naming policy, and is unit
tested there. It is applied to the three places that draw an intent — the nav
row, the tab label and the rail chip — and to nothing else. A pane's occupant,
a board's configured name and an exit explanation are identifiers or sentences,
and the head is what identifies those.
