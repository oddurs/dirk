---
id: 191
title: A conflict marker can be committed and shipped
type: bug
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p2
area: packaging
effort: s
---

## What happens

A merge conflict resolved badly leaves `<<<<<<<`, `|||||||`, `=======` and
`>>>>>>>` in the file, and nothing in the gate objects unless the file is Rust.

`cargo fmt --check` catches it in `src/` and `tests/` because the markers are
not valid Rust. Everything else is unguarded: `mandoc -T lint doc/dirk.1`
accepted a manual page with three conflict markers in the middle of its
COMMANDS section and reported nothing, and the page was committed that way.

The same hole is open for `README.md`, `NEWS`, `config.toml` examples, the
workflow files, `cairn/items/*.md` and the site's content.

## What should happen

A conflict marker never reaches a commit. It is never intentional, it is
trivial to detect, and it is the kind of mistake that gets past a reviewer
because the eye reads the surrounding prose and skips the noise.

## Reproduction

1. Put `<<<<<<< HEAD` in `doc/dirk.1`.
2. `make check` — passes.

## Acceptance criteria

- [x] A conflict marker anywhere in the tree fails the gate
- [x] It says which file and which line
- [x] The check is in the pre-push hook as well as in CI

## 2026-09-08

`git grep` for a marker at the start of a line, over tracked files, in `make
lint` and in the pre-push hook. The gate is a thing you run; the hook is a thing
that runs.

Anchored to the line start and requiring a space or an end after the seven
characters, so prose about conflict markers — this item, for instance — is not
one. `Makefile` and the hook exclude themselves, since both contain the pattern
they are looking for.

Found the way these things are found: a badly resolved cherry-pick put three
markers in the middle of `doc/dirk.1`, `mandoc -T lint` passed it, and it was
committed. `cargo fmt` catches this in Rust and nothing was catching it anywhere
else.
