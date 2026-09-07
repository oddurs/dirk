# Contributing to dirk

Everyone is welcome here, on the terms in [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

This is the social half: what a good change looks like, and what the code is
trying to be. [HACKING](HACKING) is the mechanical half — the loop from a fresh
clone to a merged pull request, and how a release is cut.

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

```sh
make setup            # once per checkout: the git alias and the hooks
git work start 7      # claims the item, branches, opens a worktree
```

`git work start` claims the item so nobody duplicates your work, branches from
`origin/main`, and gives the branch a worktree of its own — so two people, or
two agents, can work at once without sharing a build directory. It prints the
path; everything after that happens there.

`cairn release 7` hands the item back if you change your mind, and
`git work list` says what you have in flight.

## While you work

Record what you learn in the item rather than in a scratch file:

```sh
cairn set 7 labels+=needs-linux-testing
cairn edit 7
```

## Before you send it

```sh
git work ship
```

That runs `make check` — formatting, clippy with warnings denied, and the full
test suite, all of which must pass — then pushes, opens the pull request, and
turns auto-merge on so it lands itself when CI goes green. `git work land`
closes the item and removes the worktree afterwards.

Doing it by hand is fine too; `make check` is the only part that is not
optional.

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

## Attribution

The history says who decided something, not which program typed it. Commits
carry no `Co-authored-by` trailer naming a tool and no generated-with footer;
`.githooks/commit-msg` strips them, and CI fails a branch that has one. Human
co-authors are welcome and are left alone.

## Licence

dirk is free software under the GNU General Public License, version 3 or later.
By contributing you agree that your changes are licensed on the same terms. You
keep the copyright in what you wrote; add yourself to `AUTHORS` in your first
accepted change.
