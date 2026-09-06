# dirk

[![ci](https://github.com/oddurs/dirk/actions/workflows/ci.yml/badge.svg)](https://github.com/oddurs/dirk/actions/workflows/ci.yml)
[![license: GPLv3+](https://img.shields.io/badge/license-GPLv3+-blue.svg)](COPYING)

A terminal multiplexer that knows what its sessions are for.

```
┌──────────────┬──────────────────────────────────────────────────┐
│  P A G E S   │                                                  │
│  1 • ptop    │                                                  │
│  2   lazygit │                                                  │
│  3   cairn   │                the focused pane                  │
│              │                                                  │
│ P R O J E C… │                                                  │
│ ▾ dirk       │                                                  │
│   * mux core │                                                  │
│   · shell    │                                                  │
│   + workspace│                                                  │
│ ▸ smali    3 │                                                  │
│              │                                                  │
│ o open proj… │                                                  │
├──────────────┴──────────────────────────────────────────────────┤
│ ◆ dirk   ▊1 mux core  ▏2 shell  ▏3 smali        3 spaces  14:22 │
└─────────────────────────────────────────────────────────────────┘
```

tmux gives you panes and asks you to remember what is in them. dirk starts from
the opposite end: the sidebar is the product, and the panes hang off it.

## Three ideas, and keeping them apart is the whole design

**The sidebar is the nav, and it lists what is open.** Not what exists —
`~/Code` has ninety directories in it and a list of ninety things is a file
browser. A project appears once it has a workspace, and disappears when its last
one closes. `o` opens something new.

**Pages are not workspaces.** ptop, lazygit and a cairn board are singletons
with no project: there is one of each, they sit above the rule, and they are
spawned the first time you open one rather than all running in the background so
that one of them can occasionally be glanced at.

**Names come from the work, not from you.** A coding agent already publishes a
summary of what it is doing as its terminal title. dirk reads it and names the
workspace after it — continuously, with no model call and no API key. The
interesting problem was never *generating* a name; it is deciding **when** one
should change, which is what `src/name.rs` does and nothing else.

## Clickable first

Every row in the sidebar, every chip in the rail and every pane is a click
target. The hit map is built by the renderer as it paints, so there is no second
layout pass that can disagree with the first.

Clicks reach the program inside a pane too — dirk asks the pane's own terminal
state whether it turned mouse reporting on. So a click means "focus this pane"
in a shell and "click this line" in lazygit, with no mode to remember.

## Keys

The prefix is <kbd>Ctrl</kbd>+<kbd>Space</kbd>. <kbd>Ctrl</kbd>+<kbd>a</kbd> and
<kbd>Ctrl</kbd>+<kbd>b</kbd> are both load-bearing in every shell line editor;
Ctrl-Space is NUL, which nothing sends on purpose. Press it twice to send a
literal one through.

| After the prefix | |
| --- | --- |
| `n` | new workspace in this project |
| `o` | open a project |
| `x` | close the focused pane |
| `\|` `v` | split into columns |
| `-` `s` | split into rows |
| `;` | next pane in this workspace |
| <kbd>Tab</kbd> `j` `k` | next / previous workspace |
| `1` `2` `3` | jump to a page |
| `d` | hide the sidebar |
| `q` | quit |

Everything else goes straight through to the program in the pane.

## Configuration

dirk runs with no config file. `~/.config/dirk/config.toml` overrides what it
names and leaves the rest alone.

```toml
projects_root = "~/Code"     # where `o` looks
sidebar_width = 28
scrollback    = 5000
shell         = ""           # empty means $SHELL

[brand]
mark = "◆"
name = "dirk"

# A page whose program is not on PATH is dropped at startup rather than left to
# fail on first open.
[[pages]]
title   = "ptop"
command = ["ptop"]
key     = "1"

[[pages]]
title   = "cairn"
command = ["cairn", "board"]
key     = "3"

[naming]
enabled          = true
debounce_ms      = 1200      # how long a title must hold still
min_interval_ms  = 15000     # floor between two renames of one workspace
```

## Naming, in detail

Ported from [namesync](https://github.com/oddurs/namesync), which did this as a
herdr plugin over a socket. Inside dirk the title arrives from the pane's own
OSC callback and the label is a struct field, so the plugin's daemon, client,
state store and sinks all disappear. The policy is unchanged:

| Rule | Behaviour |
| --- | --- |
| Hand-written names win | If a label is not the one dirk last wrote, a human wrote it. Never touched again. |
| Defaults are adoptable | `w3`, `tab 2`, the bare repo name — nobody chose these, so they get claimed. |
| Rewordings are not new intent | "naming plugin" → "naming plugins" scores 1.0 on stemmed token overlap and is skipped. |
| Settle before committing | Titles churn early in a turn; a rename waits for the intent to hold still. |
| One rename per workspace per interval | So a fast session cannot strobe the sidebar. |
| Junk is never a name | Shell prompts, bare paths, echoed commands and the plain repo name are rejected. |
| No guessing across agents | A workspace holding two panes has no single intent. |

## Build

```console
$ cargo build --release        # ~6s, 1.0 MB
$ cargo test                   # 16 tests, under a second
$ ./target/release/dirk
```

The unit tests cover the naming policy, which is pure. The rest is a terminal
talking to a terminal, so `tests/smoke.rs` gives dirk a real pty, reads what it
paints and drives it with real keystrokes.

## What it is built on

The hard part of a multiplexer is terminal emulation, and it is a library.

| | |
| --- | --- |
| [`vt100`](https://crates.io/crates/vt100) | the grid, scrollback and escape parsing |
| [`portable-pty`](https://crates.io/crates/portable-pty) | spawning a child on a pty |
| [`ratatui`](https://crates.io/crates/ratatui) + [`crossterm`](https://crates.io/crates/crossterm) | the chrome |

What is left for dirk to own is the window manager over the top: the tree, input
routing, the hit map, the render composition and the naming policy.

## Contributing

The backlog is the contribution guide: `cairn next` says what is ready to work
on, and each item carries the reasoning that produced it. See
[CONTRIBUTING.md](CONTRIBUTING.md), and [NEWS](NEWS) for what has changed.

```console
$ make check      # fmt, clippy and the full suite — the gate CI enforces
$ make shot       # print what dirk currently paints, as plain text
```

## Licence

dirk is free software: you can redistribute it and/or modify it under the terms
of the GNU General Public License as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version. See
[COPYING](COPYING).

It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE.

## Status

v0.1 is a single process. Close the terminal and the work goes with it, so
[herdr](https://herdr.dev) stays installed for anything long-running. Detach and
reattach is [item 0007](cairn/items); the roadmap is in
[ROADMAP.md](ROADMAP.md), managed with [cairn](https://github.com/oddurs/cairn).

The other honest gap: the sidebar's state glyph is inferred from whether a pane
has ever published a title, so `blocked` — the one state that is waiting on you
— is never actually shown. That is item 0009.
