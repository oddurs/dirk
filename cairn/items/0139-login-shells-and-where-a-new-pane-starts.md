---
id: 139
title: Login shells, and where a new pane starts
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: config
effort: s
---

## Problem
Two defaults that are wrong often enough to be worth a knob each.

New panes start a non-login shell, so on macOS `/usr/libexec/path_helper` and
Homebrew's initialisation never run and `PATH` in a dirk pane is missing entries
it has everywhere else. That is a bug people diagnose as dirk being broken.

And a new workspace always inherits the source pane's directory, with no way to
say otherwise.

## Proposal
`[terminal] shell_mode = "auto" | "login" | "non_login"`, where `auto` means
login on macOS and the current behaviour elsewhere.

`[terminal] new_cwd = "follow" | "home" | "current" | "<path>"`, defaulting to
`follow`. An explicit `--cwd` still wins.

## Acceptance criteria
- [ ] `shell_mode`, defaulting to `auto`; documented as a PATH fix, not a preference
- [ ] `new_cwd` with all four forms
- [ ] `--cwd` from the CLI takes precedence
- [ ] Boards and commands keep their existing launch path
