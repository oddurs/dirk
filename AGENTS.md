# Working on dirk

dirk is a terminal multiplexer whose sidebar is the product. It is written
mostly by coding agents, which is also what it is for, so this file is the
brief. Read it before you change anything.

`HACKING` has the mechanical detail; this is what has to be in your head.

## The loop

    make setup                # once per checkout: the git alias and the hooks
    git work start <ID>       # claim the item, branch, open a worktree
    cd <the path it printed>
    ... work ...
    make check                # the gate CI enforces
    git work ship             # push, pull request, merge itself when green
    git work land             # close the item, remove the worktree
    git work drop             # changed your mind: hand the item back instead

Every branch gets a worktree of its own, so two agents can work at once without
sharing a build directory. Never `git switch` in a checkout someone else may be
building in.

`make check` is `cargo fmt --check`, `cargo clippy --all-targets -D warnings`
and the full test suite. It must pass. Do not ship around it and do not use
`--no-verify`.

## What the code is trying to be

**The hard part is a library.** Terminal emulation is `vt100`'s job and pty
handling is `portable-pty`'s. What dirk owns is the window manager over the
top: the tree, input routing, the hit map, render composition, and naming. Work
that belongs to a dependency should move there rather than grow here.

**Chrome is not content.** The sidebar and the bar paint their own ground;
panes paint nothing of their own and carry whatever the program inside drew.
That difference is the only reason the navigation reads as navigation.

**Clicks are registered as things are drawn.** The hit map is a by-product of
rendering, not a model of it, so there is no second layout pass that can
disagree with the first. A new clickable thing registers its own rectangle as
it paints.

## Traps that have already caught someone

- **The pty harness must be drained continuously.** `tests/smoke.rs` reads on
  its own thread, always. A test that reads only while waiting for output fills
  the buffer the moment it stops; dirk blocks in `write`; the next keystroke is
  never processed. The symptom looks exactly like dirk ignoring input.
- **A pane's output is untrusted input.** It can come from a remote host over
  ssh or from a program processing a hostile file. Escape sequences from inside
  a pane must not reach the outer terminal unfiltered.
- **Answers to the CLI are JSON, including the failures.** The caller is a
  program; prose on stderr is not something a program can branch on.
- **`print!` panics on a closed pipe.** Anything a user might pipe into `head`
  has to handle that.
- **`since` and `age` are different clocks.** `since` restarts when the state
  changes; `age` restarts when the intent changes. Confusing them makes both
  useless.

## Style

`cargo fmt` decides formatting; do not argue with it in review. Comments say
why, not what — the code already says what. A comment recording a decision, a
trap, or something tried and rejected is worth keeping; one narrating the next
line is not.

Commit subjects are lowercase, area-prefixed, and say what changed rather than
what was done: `boards: a row that reports without being opened`. The body is
prose saying why, with a GNU-style file list at the end when more than one
place changed.

## Attribution

The history says who decided something, not which program typed it. Never add a
`Co-authored-by` trailer naming a tool, and never add a generated-with footer.
`.githooks/commit-msg` strips them regardless, and CI fails a branch that
carries one — but do not make it do that work.

## Subagents and commands

`.claude/agents/` holds the specialists: `backlog`, `smoke`, `chrome`, `news`,
`reviewer`, `docs`, `site`. `.claude/commands/` holds the loop above as
`/work`, `/check`, `/ship`, `/land`, `/release`, `/backlog`.

## The website

`site/` is a workspace member: `make site` builds it, `make site-serve` serves
it. It is generated from the program — the palette from `src/theme.rs`, the
roadmap from `ROADMAP.md`, the changelog from `NEWS`, the screens from the real
binary — so **nothing on the site is a hand-copied duplicate of something in
the repository.** If you are about to type a keybinding, a colour or a version
number that already exists in the tree, generate it instead. `HACKING` has the
rest.

<!-- cairn:begin -->
## Roadmap and issues

This project tracks its roadmap and issues with `cairn`. Every item is a Markdown file under `cairn/items`, described by the schema in `cairn.toml`.

**Do not create ad-hoc TODO, PLAN or NOTES files.** Create a cairn item instead, so the work appears on the board and in the generated roadmap.

### The loop

1. `cairn next` — what is ready to start. It excludes anything blocked by unfinished dependencies and puts work already in progress first.
2. `cairn claim <ID>` — take it before you start, so no one duplicates the work. `cairn claim --next` picks and claims the top-ranked unclaimed item in one step, and prints its body so you can begin immediately.
3. Do the work, recording what you learn: `cairn set <ID> <field>=<value>`.
4. `cairn close <ID>` when it is done, or `cairn release <ID>` to hand it back.
5. `cairn check` before you report finished. It must pass.

### Commands

```sh
cairn next --json                 # ready work, ranked
cairn claim --next                # take the next ready item
cairn search <TEXT> --json        # titles, bodies and labels
cairn list --json                 # all open items
cairn list --filter 'blocked=false,priority=p0'
cairn show <ID> --json            # one item, including its body
cairn new "<TITLE>" --type <TYPE> --milestone <MILESTONE>
cairn set <ID> status=<STATUS>    # also labels+=x, or any field below
cairn close <ID>
cairn check                       # validate; run before finishing
cairn render                      # regenerate ROADMAP.md
```

### Schema

- **Types**: `feature`, `bug`, `chore`, `docs`
- **Statuses**: `backlog` (open), `planned` (open), `doing` (active), `blocked` (active), `done` (done), `dropped` (dropped)
- **`priority`**: one of p0, p1, p2, p3 — p0 is a release blocker
- **`effort`**: one of s, m, l, xl — Rough size, not an estimate
- **`area`**: free text — Subsystem this touches
- **Milestones**: `v0.1` (due 2026-12-01), `v1.0` (due 2027-03-01), `later`
- **Saved views** (`cairn list --view NAME`): `now`, `next`, `triage`

### Rules

1. Before starting work, find or create the item and set it to an active status.
2. Use the fields above rather than inventing new ones; add new fields to `cairn.toml` first.
3. Never hand-edit the generated roadmap file — change items and run `cairn render`.
4. `cairn check` must pass before the work is considered done.

<!-- cairn:end -->
