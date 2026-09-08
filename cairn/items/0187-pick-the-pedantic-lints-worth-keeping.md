---
id: 187
title: Pick the pedantic lints worth keeping
type: chore
status: done
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

- [x] The chosen lints are in `Cargo.toml`, each with its reason
- [x] The tree is clean under them
- [x] CI enforces them

## 2026-09-08

Read rather than obeyed, and most of what the item nominated did not survive
the reading.

**`unused_self`, 22 of them — declined.** The item guessed these were methods on
`App` that never touch it. Every one is in `theme.rs`: a zero-sized `Theme` whose
accessors take `self` so that `THEME.panel()` reads the way it does, and so a
theme that carries a palette can arrive without changing every call site. That
is deliberate and the lint is wrong about it.

**`match_same_arms`, 3 — declined.** All three are distinct cases that happen to
need the same nothing. `(Some(_), Some(_))` and `(None, None)` are different
situations; `Waiting if seen` and `Said(Done) if seen` are different
observations. Merging them saves two lines and loses what the arms were
documenting.

**`assigning_clones`, 9 — declined.** `clone_from` is measurably better in a
loop over large values and this is not that; what it is here is a less readable
line for no gain anybody could measure.

**`cast_possible_truncation`, 42 — declined.** Spot-checked: the ones in the
drawing code are bounds-checked before the cast rather than after, which is the
right order and is what the lint cannot see.

**`implicit_clone`, 7 — taken.** `to_path_buf()` on a `PathBuf` and
`to_string()` on a `String` both say "convert" where what happens is a copy.

The one worth more than all of them was not on the list. The audit's own note
said there is not one `unwrap` outside a test in `src`, and nothing was keeping
it that way — so `unwrap_used` and `expect_used` are denied now, with
`clippy.toml` allowing them where a panic is an assertion. The tree needed no
changes at all: it was already true, and now it stays true. `todo`,
`unimplemented` and `dbg_macro` went in beside them on the same argument.

`0191` is here too, because it is the same question. A conflict marker reached a
manual page during this run of work and `mandoc -T lint` had nothing to say
about it — `cargo fmt` only sees the ones in Rust.
