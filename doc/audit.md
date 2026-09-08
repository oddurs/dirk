# Auditing dirk

A pass over the whole tree looking for what is wrong with it rather than for
what is missing from it. The roadmap answers "what should dirk do next"; this
answers "what is dirk carrying".

Run it when a run of feature work has landed — three milestones is about right —
and file what it finds. Findings go in cairn like everything else. An audit
whose output is a document nobody opens again was a way of feeling thorough.

## The passes

Each is a question, a way to ask it, and what a bad answer looks like. None of
them needs judgement to run; all of them need judgement to read.

**Does it build where it ships?** `cargo check --target T` for every target in
`.github/workflows/release.yml`. The tag is the trigger for a release build, so
a target nobody checks is a target that fails during publication.

**What panics?** `grep` for `unwrap`, `expect`, `panic!`, indexing, and check
each against `mod tests` — a panic in a test is an assertion. Production code
that can panic in a session holding somebody's shells is the highest-cost defect
this program has.

**What is silently swallowed?** `let _ =`, `.ok()`, `else { return }`,
`unwrap_or_default`. Every one is a decision that something is not worth
reporting. Most are right. The ones that are wrong look identical, so read them.

**What leaks?** Collections on long-lived state that are pushed to and never
pruned; threads that block with no timeout; sockets whose owner can go away.
The test is not to read the code but to run it and count: start the session,
do the thing a hundred times, count threads and descriptors, do it again.

**What is duplicated?** Functions with the same shape in different files. Grep
for a distinctive line — an arithmetic expression, a constant — and see how many
files it appears in. Two copies of twenty lines is where one copy gets a fix and
the other does not.

**How big is the biggest thing?** Longest functions, longest files, and the
ratio of lines to unit tests per file. A file with thousands of lines and one
test is not under-tested by accident; it is untestable, and the size is why.

**Where do the docs disagree with the program?** For anything with a table in
the source — commands, actions, settings — check every entry appears in
`doc/dirk.1` and the README. Prefer a test to a checklist: the check that runs
is the one that keeps running.

**What runs on the hot path?** Read what the event loop calls on every turn and
follow it down. Diagnostic code reached from the fast path is the usual find:
it allocates to explain itself and the caller keeps one field.

**What would a stricter lint say?** `cargo clippy -- -W clippy::pedantic`, then
read rather than obey. Most of it will disagree with the house style on purpose.
The point is the handful that do not.

**Does the test suite depend on where it is run?** Ambient git state, the
current directory, the shell that happens to be `/bin/sh`, a program that
happens to be installed. These fail on one machine and pass on another, which
costs more than failing everywhere.

## What this pass found, September 2026

Twelve items, `0176`–`0187`. One of them mattered more than the rest: dirk had
not compiled for `aarch64-unknown-linux-gnu` — one of five release targets —
since the window-title work landed, and would have been found by the tag that
published it.

Worth writing down, so it is not re-argued: several things were looked at and
were fine. There is not one `unwrap` outside a test in the whole of `src`. No
`lock().unwrap()`. Every dependency is justified in `Cargo.toml`, several with
the reason they were preferred to writing it by hand. Events are coalesced
before the loop does its work, so a fast pane produces one redraw. The
`u16` casts in the drawing code are bounds-checked before the cast, not after.
