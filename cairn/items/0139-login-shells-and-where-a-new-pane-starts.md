---
id: 139
title: Login shells, and where a new pane starts
type: feature
status: done
milestone: v0.8
created: 2026-09-07
updated: 2026-09-08
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
- [x] `shell_mode`, defaulting to `auto`; documented as a PATH fix, not a preference
- [x] `new_cwd` with all four forms
- [x] `--cwd` from the CLI takes precedence
- [x] Boards and commands keep their existing launch path

## 2026-09-08

`-l` rather than an argv[0] beginning with a dash. The dash is the older
convention and is what portable-pty reserves for the shell it picks itself;
there is no way to set argv0 independently through its builder. Every shell
somebody would set here takes `-l`, and one that does not wants `non_login`.

`new_cwd` applies where a pane is made *beside* another one — splitting, and a
new tab. A workspace opens in its project because that is what a workspace is:
its checkout is part of what the nav means by one, and `home` there would break
the model rather than the habit.

`Pane::spawn` grew a `Setup` on the way, because eight arguments is too many and
three of them were numbers sitting next to each other waiting to be swapped.

And it turned up a real one. Turning login shells on by default on macOS means
`/etc/bashrc` sets the window title to the working directory, so every plain
shell pane started announcing where it was — and naming believed it. A title
that is the pane's own directory, or a `host:place` prompt without the `@` the
older rule looked for, is now location rather than intent. That was always true;
it was just not reachable by default before.

It also exposed a race in an existing test: it waited for `side` to appear
anywhere on screen, and a pane's prompt carries its directory — `repo-hang-side`
— so the wait was satisfied by the shell before git had answered. It now asks
the session for the branch.

CI then failed two nav tests that assert on drawn columns. A login shell brings
`/etc/profile`, `/etc/bashrc` and somebody's prompt into the picture, and a
prompt that publishes its directory as a window title becomes a workspace label
— a label of a different length moves every column after it. The suite now pins
`non_login`, for the same reason it already sets `SHELL` and clears the `GIT_*`
variables: what is being tested is dirk, not the runner's login files. The test
that is about login shells writes its own configuration.

The smoke harness needed the same, and appended rather than prepended: a section
header written before somebody's top-level keys swallows them into it, the file
is refused for unknown fields, and the test runs on the defaults in silence —
which is exactly what it looked like the first time.
