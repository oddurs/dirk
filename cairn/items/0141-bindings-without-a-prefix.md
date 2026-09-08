---
id: 141
title: Bindings without a prefix
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: config
effort: m
---

## Problem
Once keys are configurable (`0014`) somebody will want to skip the prefix. The
hard part is not binding a chord, it is knowing which chords survive: the
operating system, the outer terminal and the program in the pane all take a cut
before dirk sees anything, and picking wrong produces a binding that silently
does nothing.

## Proposal
Let any action take a list of bindings, so the prefix form and a direct chord
can both be live. Then do the work that makes it usable: check the chords
against what the common terminals and desktops already claim, ship a documented
safe set, and say which families are traps and why — `ctrl+j` is Enter to every
shell, plain `alt` composes characters on macOS.

herdr surveyed ten terminals and two desktops and concluded `ctrl+alt` is the
one family nearly nothing claims. That survey is the deliverable here as much as
the code is.

Depends on `0014`.

## Acceptance criteria
- [ ] An action accepts a list of bindings; prefix and direct forms coexist
- [ ] A documented safe set, with the known conflicts listed
- [ ] A binding that cannot be received is reported, not silently dead
- [ ] Direct chords do not shadow the pane's program by accident
