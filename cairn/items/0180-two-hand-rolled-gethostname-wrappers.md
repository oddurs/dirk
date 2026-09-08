---
id: 180
title: Two hand-rolled gethostname wrappers
type: chore
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p3
area: config
effort: s
---

## Problem

`src/config.rs:271` and `src/main.rs:804` both wrap `libc::gethostname`. Both
take 256 bytes, both stop at the first NUL, both cut at the first dot and both
explain why. They differ in two ways and only one of them matters:

- `config.rs` returns `Option<String>`, `main.rs` returns `String`;
- `config.rs` uses `[u8; 256]` with `.cast()`, which is `c_char` everywhere;
  `main.rs` uses `[i8; 256]`, which is `c_char` on x86-64 and Darwin and not on
  aarch64 Linux.

The second difference is a build break on a shipped target, filed separately.
This item is the reason it could happen: the same twenty lines exist twice, one
copy is portable and the other is not, and nothing connects them.

Two `unsafe` blocks where one would do is also two things to review.

## Proposal

One `pub fn hostname() -> Option<String>` in `config.rs`, which already has the
better of the two. `main.rs` takes `.unwrap_or_default()`.

## Acceptance criteria

- [ ] One `gethostname` call site in the tree
- [ ] It is the portable one
- [ ] The window title still says the short host name
