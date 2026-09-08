---
id: 179
title: The manual page has fallen behind the commands
type: docs
status: done
created: 2026-09-08
updated: 2026-09-08
priority: p2
area: docs
effort: s
---

## Problem

`doc/dirk.1` documents the CLI by noun and verb, and it is missing:

- **`tab`** — the whole noun. `list`, `new`, `focus`, `rename`, `close`, landed
  in v0.8.
- **`worktree`** — the whole noun. `list`, `add`, `remove`.
- **`pane move`**, also v0.8.
- **`session quit`**.

That is nine of forty-one commands with no entry in the page that is supposed to
be the interface's documentation.

CI runs `mandoc -T lint doc/dirk.1`, which checks that the page parses. Nothing
checks that it says anything about the program.

## Proposal

Write the missing entries, and add the guard that would have caught it.

`src/api.rs` already has the sibling of this test: it scans the source for
command literals that something branches on and fails when one is not in
`COMMANDS`. The same shape works here — every `COMMANDS` entry's noun and verb
must appear in the page's `COMMANDS` section.

A test rather than a lint, because it needs to read `COMMANDS`, and because the
failure should name the command that is missing.

## Acceptance criteria

- [x] `tab` and `worktree` are documented, with their verbs
- [x] `pane move` and `session quit` are documented
- [x] A test fails when a command is added to `COMMANDS` and not to the page

## 2026-09-08

Scoped to the page's own COMMANDS section, which is the part that matters.
Searching the whole page would pass on nothing: `tab` is also a key, `close` is
also prose, and a word appearing somewhere in a manual is not a command being
documented in it. Both halves of a name are checked, so a noun that arrives with
five verbs cannot be half-documented either.

Just enough roff is undone to find a word: `send\-keys` is written with the
hyphen escaped so it is not a line break, and a scan looking for `send-keys`
would not find it.

Confirmed the other way round — the entry for `tab` was renamed and the test
failed, naming the command it could not find.
