---
id: 176
title: dirk does not build for aarch64 Linux
type: bug
status: backlog
milestone: v1.0
created: 2026-09-08
updated: 2026-09-08
priority: p0
area: packaging
effort: s
---

## What happens

`cargo check --target aarch64-unknown-linux-gnu` fails:

    error[E0308]: mismatched types
      --> src/main.rs:807:41

`hostname()` in `src/main.rs` holds its buffer as `[i8; 256]` and hands
`buf.as_mut_ptr()` — a `*mut i8` — to `libc::gethostname`, which takes
`*mut c_char`. On x86-64 and on Darwin `c_char` is `i8` and this compiles. On
aarch64 Linux `c_char` is `u8` and it does not.

`aarch64-unknown-linux-gnu` is one of the five targets
`.github/workflows/release.yml` builds. The tag is the trigger, so the first
time anyone would find this is halfway through publishing a release.

## What should happen

It builds everywhere it is shipped.

`src/config.rs` already has the portable form of exactly this function, six
lines away from being the same code: `[u8; 256]` with `.cast()`, which is
`c_char` on every target. There should be one of these, not two — see the item
about the two hostname wrappers.

## Reproduction

1. `rustup target add aarch64-unknown-linux-gnu`
2. `cargo check --target aarch64-unknown-linux-gnu`

## Acceptance criteria

- [ ] `cargo check` passes for every target in `release.yml`
- [ ] There is one `gethostname` wrapper in the tree, not two
