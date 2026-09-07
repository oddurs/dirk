---
description: Run the gate CI enforces, and fix what it finds
allowed-tools: Bash(make:*), Bash(cargo:*), Bash(mandoc:*), Bash(shellcheck:*), Read, Edit, Grep, Glob
---

Run `make check`: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`
and the full test suite.

Fix what it finds, in this order:

1. **Test failures first.** A failing test is a fact about the program. Read
   what it asserted and what it got before you change either. If a test hangs,
   suspect the pty drain in `tests/smoke.rs` before you suspect dirk — the
   harness must read continuously or dirk blocks in `write` and stops
   processing keys.
2. **Clippy.** Take the suggestion unless it makes the code worse; if it does,
   say why rather than silencing it with an attribute.
3. **Formatting.** `cargo fmt`. Do not argue with it.

Never weaken a test to make it pass. Never add `#[allow]` without a comment
saying why the lint is wrong here.

Report what failed, what you changed, and the final state.
