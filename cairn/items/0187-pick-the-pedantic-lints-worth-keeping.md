---
id: 187
title: Pick the pedantic lints worth keeping
type: chore
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p3
area: packaging
effort: m
---

## Problem

CI runs `cargo clippy --all-targets -- -D warnings`, which is the default set.
Running the pedantic set over the tree finds about three hundred more. Most are
noise here — `match` on a boolean is an idiom this codebase uses deliberately,
and `-W clippy::doc_markdown` disagrees with prose. Some are not:

- **`unused_self`**, 22 of them: methods on `App` that never touch `self`. Each
  one is a free function wearing a method's clothes, and each is a thing that
  cannot be tested without an `App`. Symptom of the same problem as the
  `main.rs` item, and a cheap way to chip at it.
- **`cast_possible_truncation`**, 42: `usize as u16`, mostly screen
  coordinates that genuinely cannot exceed a `u16` and a few that are worth a
  second look.
- **`assigning_clones`**, 7, and **`redundant_clone`**: small and real.
- **`match_same_arms`**, 3: arms that are identical, which is either a
  simplification or a case somebody meant to handle differently.

## Proposal

Choose. Add the lints worth keeping to `[lints.clippy]` in `Cargo.toml`, at
`warn` or `deny`, with the reason next to each — the same way the dependencies
carry theirs. Then fix what they find, in its own branch.

Do not turn on `pedantic` wholesale. A lint set nobody agrees with is a lint
set people learn to override, and this project's style has opinions that
disagree with several of them on purpose.

## Acceptance criteria

- [ ] The chosen lints are in `Cargo.toml`, each with its reason
- [ ] The tree is clean under them
- [ ] CI enforces them
