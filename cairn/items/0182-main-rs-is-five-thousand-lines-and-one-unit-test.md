---
id: 182
title: main.rs is five thousand lines and one unit test
type: chore
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p2
area: mux
effort: l
---

## Problem

`src/main.rs` is 4,911 lines — a fifth of the program — and holds one `#[test]`.
Everything in it is covered, if at all, through the pseudo-terminal suites,
which take thirty seconds and can only assert on what reaches a screen.

What is in there is not one job:

- argument parsing and the `Mode` it resolves to;
- `serve`, `alone`, terminal setup and restore;
- the `App` struct — about ninety fields — and its event loop;
- the API request dispatcher, `ask`, which is **585 lines** of one `match`;
- key handling for eight modes, each its own `fn *_key`;
- mouse handling, hit testing and pointer selection;
- rendering, composition and the image placement pass;
- agents, badges, intents, repositories and the rename pass.

`App::ask` is the sharpest instance. It is the whole socket API in one
function, it is the surface most likely to be extended, and every addition to
it makes the function harder to read and no easier to test.

The size is not the problem in itself. The problem is what the size prevents: a
585-line method on a ninety-field struct cannot be unit tested, so the only way
to check any of it is to start a terminal.

## Proposal

Not a rewrite. Move the seams that are already there:

- `ask` splits by noun — `api::pane`, `api::workspace`, `api::agent` — each
  taking what it needs rather than `&mut App`. `src/api.rs` already owns the
  table; this is the other half of it.
- the `*_key` handlers are a mode dispatcher; they belong with the modes.
- `main` keeps argument parsing, mode selection and the event loop.

Do it a piece at a time, each piece its own branch, each one leaving behind the
unit tests that the extraction makes possible. A piece that does not produce a
test it could not have had before is a piece that was moved for tidiness.

## Acceptance criteria

- [ ] `App::ask` is no longer one function
- [ ] The API is reachable from a unit test without starting a terminal
- [ ] `main.rs` holds argument parsing, mode selection and the loop
- [ ] Each extraction lands with tests it enabled
