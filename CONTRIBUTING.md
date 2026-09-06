# Contributing to dirk

dirk tracks its own roadmap with [cairn](https://github.com/oddurs/cairn), so
the backlog is the contribution guide.

```sh
cairn next            # what is ready to work on
cairn next --blocked  # and what is waiting on something
cairn show 7          # the reasoning behind an item
```

Items carry the thinking that produced them — the problem, the proposal, what
was weighed, and acceptance criteria you can check yourself. If an item's body
does not tell you enough to start, that is a bug in the item; say so.

## Before you start

Claim the item, so nobody duplicates your work:

```sh
cairn claim 7
```

`cairn release 7` hands it back if you change your mind.

## While you work

Record what you learn in the item rather than in a scratch file:

```sh
cairn set 7 labels+=needs-linux-testing
cairn edit 7
```

## Before you send it

```sh
make check
```

That runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
the full test suite. All of it must pass.

## Testing a program that draws a terminal

The naming policy in `src/name.rs` is pure, and it is unit tested directly.
Everything else in dirk is a terminal talking to a terminal, and the only
honest way to test that is to give it one. `tests/smoke.rs` spawns the real
binary on a real pseudo-terminal, reads what it paints, and drives it with real
keystrokes. New behaviour that a user can see belongs there.

One trap, because it has already caught someone: the harness drains the
pseudo-terminal on its own thread, always. A test that reads only while waiting
for output fills the buffer the moment it stops, dirk blocks in `write`, and
the next keystroke is never processed — which looks exactly like dirk ignoring
your input.

`cargo run --example shot` prints what dirk currently paints, as plain text.
It is the fastest way to see whether a change to the chrome did what you meant.

## What the code is trying to be

Three things worth knowing before changing much:

**The hard part is a library.** Terminal emulation is `vt100`'s job and pty
handling is `portable-pty`'s. What dirk owns is the window manager over the
top: the tree, input routing, the hit map, render composition and naming. Work
that belongs to a dependency should move there rather than growing here.

**Chrome is not content.** The sidebar and the bar paint their own ground;
panes paint nothing of their own and carry whatever the program inside drew.
That difference is the only reason the navigation reads as navigation.

**Clicks are registered as things are drawn.** The hit map is a by-product of
rendering, not a model of it, so there is no second layout pass that can
disagree with the first. A new clickable thing registers its own rectangle as
it paints.

## Style

`cargo fmt` decides formatting; do not argue with it in review. Comments should
say why, not what — the code already says what. A comment that records a
decision, a trap, or a thing that was tried and rejected is worth keeping; one
that narrates the next line is not.

## Licence

dirk is free software under the GNU General Public License, version 3 or later.
By contributing you agree that your changes are licensed on the same terms. You
keep the copyright in what you wrote; add yourself to `AUTHORS` in your first
accepted change.
