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
- [x] An action accepts a list of bindings; prefix and direct forms coexist
- [x] A documented safe set, with the known conflicts listed
- [x] A binding that cannot be received is reported, not silently dead
- [x] Direct chords do not shadow the pane's program by accident

## 2026-09-08

A bare word stays what it always was — a key after the prefix — so every
existing configuration means what it meant. `prefix+n` says the same thing
explicitly, and anything with a `+` and a modifier dirk knows is a chord.

Answered last, after every mode holding the keyboard, so a chord typed into the
palette's filter is a letter rather than a command. That also means a direct
chord cannot reach past copy mode or a question, which is the behaviour somebody
would expect from a mode.

The survey is the deliverable as much as the code. dirk cannot detect a chord
the desktop ate — nothing arrives — so the list of families that are spoken for
is written down and complained about at load. Two more that *do* reach dirk are
worth refusing too: `ctrl+j` is Enter and `ctrl+i` is Tab to every program in
every pane, so binding one takes that key away from all of them.
