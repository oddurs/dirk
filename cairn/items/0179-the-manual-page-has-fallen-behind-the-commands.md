---
id: 179
title: The manual page has fallen behind the commands
type: docs
status: backlog
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

- [ ] `tab` and `worktree` are documented, with their verbs
- [ ] `pane move` and `session quit` are documented
- [ ] A test fails when a command is added to `COMMANDS` and not to the page
