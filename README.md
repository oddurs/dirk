# dirk

[![ci](https://github.com/oddurs/dirk/actions/workflows/ci.yml/badge.svg)](https://github.com/oddurs/dirk/actions/workflows/ci.yml)
[![license: GPLv3+](https://img.shields.io/badge/license-GPLv3+-blue.svg)](COPYING)

A terminal multiplexer that knows what its sessions are for.

```
┌────────────────────────────────┬────────────────────────────────────┐
│ layouts                      4 │ ─ roadmap ──────── exited · r resta│
│ 1 • Overview                   │  v0.2  The nav                     │
│ 2   ptop                       │   [####################] 100%      │
│ 3   lazygit                    │                                    │
│ 4   cairn                      │                                    │
│   ↵ open                       │ ─ ptop ──────────────── ─ board ───│
│                                │ CPU 9.5%  MEM 78.0%     0040 Search│
│ spaces                       2 │  14 cores ▃▂▂▁ ▂▁▁▁     0041 Tabs: │
│ ▾ dirk                         │ ── timeline ─────────   0042 Comman│
│   * 1 Building the mux core now│  25 ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀   0043 Pane z│
│       main                     │ CPU ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀   0044 Create│
│ ▸ · 2 Reading the grid ⑂   1d  │                                    │
│       feat/packaging           │                                    │
│   + workspace                  │                                    │
│   n new  ·  o project          │                                    │
│                                │                                    │
│ agents               attention │                                    │
│   * Building the mux core  now │                                    │
│   ↵ go  ·  s sort              │                                    │
├────────────────────────────────┴────────────────────────────────────┤
│ ◆ dirk  ▊1 mux core  ▏2 reading      ! 1  + 2   2 spaces · 14:22   ✕ │
└─────────────────────────────────────────────────────────────────────┘
```

tmux gives you panes and asks you to remember what is in them. dirk starts from
the opposite end: the sidebar is the product, and the panes hang off it.

## Three ideas, and keeping them apart is the whole design

**The nav is three lists, and the order is the argument.** Layouts are places
you go, spaces are where work lives, agents are what is asking for you.
Attention flows down the column. The same workspace appears under spaces and
under agents — that is not duplication: spaces answers "what is open, and
where", agents answers "what needs me", and they sort differently for exactly
that reason.

**The nav lists what is open.** Not what exists —
`~/Code` has ninety directories in it and a list of ninety things is a file
browser. A project appears once it has a workspace, and disappears when its last
one closes. `o` opens something new.

**Layouts are not workspaces — except that they are.** A layout is a named
arrangement of programs: one system monitor, one dashboard. There is one of
each, they sit above the rule, and they are built the first time you open one
rather than all running in the background so that one of them can occasionally
be glanced at.

Structurally a layout *is* a workspace — panes, a split tree, a focused pane and
a name is the whole of one — so there is no second code path for them. A
single-program layout and a five-pane dashboard differ only in how many leaves
they have.

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
| `d` | hide the nav |
| `u` | release a held name, so naming may claim the workspace again |
| `r` | restart a stopped pane |
| `w` | give the nav the keyboard — `j` `k` to move, Enter to go, Escape back |
| `q` | quit |

The `✕` at the right of the bar quits too, and takes two clicks: it ends every
shell and every agent in the session, and it sits at the edge of the screen
where a stray click is most likely.

Everything else goes straight through to the program in the pane.

## Configuration

dirk runs with no config file. `~/.config/dirk/config.toml` overrides what it
names and leaves the rest alone.

```toml
projects_root = "~/Code"     # where `o` looks
sidebar_width = 34
scrollback    = 5000
shell         = ""           # empty means $SHELL

[brand]
mark = "◆"
name = "dirk"

# A layout whose programs are not all on PATH is dropped at startup: an entry
# that could only ever show `command not found` is worse than no entry.
[[layout]]
name    = "ptop"
command = ["ptop"]
key     = "1"

# Panes nest. `size` is lines or columns ("5"), a share ("30%"), or absent to
# take an even part of what is left. A pane runs in the directory of whatever
# was focused when the layout opened, unless it names a `cwd` of its own.
[[layout]]
name  = "Overview"
key   = "4"
split = "rows"

  [[layout.pane]]
  title   = "brief"
  command = ["smali", "brief"]
  size    = "6"

  [[layout.pane]]
  split = "cols"

    [[layout.pane.pane]]
    title   = "ptop"
    command = ["ptop"]

    [[layout.pane.pane]]
    title   = "cairn"
    command = ["cairn", "board"]
    size    = "30%"

[naming]
enabled              = true
debounce_ms          = 1200   # how long a title must hold still
min_interval_ms      = 15000  # floor between two renames of one workspace
similarity_threshold = 0.6    # above this, a new title is the same thing reworded
respect_manual_names = true   # never overwrite a name you wrote
skip_while_blocked   = true   # a blocked agent's title is the question, not the work
strip_project_prefix = true   # "ptop-adopt-lessons" under "ptop" reads "Adopt-lessons"
# Titles that are programs rather than intents. Ships with a list; setting this
# replaces it.
ignore_titles        = ["nvim", "lazygit", "claude", "htop"]
show_stale           = true   # mark an agent that has stopped saying anything new
stale_after_turns    = 6      # counted in state changes, not in minutes

# A second source of intent, for panes whose title says nothing. Off unless
# asked for. The name of the variable holding your key, never the key.
[naming.sources.llm]
enabled        = false
endpoint       = "https://api.anthropic.com/v1/messages"
model          = "claude-opus-5"
api_key_env    = "ANTHROPIC_API_KEY"
timeout_ms     = 8000
max_chars      = 4000    # how much of the screen to send
viewport_lines = 60
interval_ms    = 600000  # floor between two questions about one workspace

[naming.targets]
workspace = true
agent     = true

# A name is a template over the tokens below. An absent token leaves no gap.
[naming.templates]
workspace = "{intent}"
agent     = "{intent-slug}"

[notify]
enabled          = true
min_interval_ms  = 60000     # floor between two interruptions about one space
```

### The tokens a name is made of

| | |
| --- | --- |
| `project` `branch` `worktree` | which checkout this is |
| `n` | the workspace's number — display, not identity |
| `intent` `intent-slug` | what the agent says it is doing |
| `agent` `agents` | the kind, and a count when there is more than one |
| `since` | how long in the **current state** — who has been blocked longest |
| `age` | how long on the **current intent** — what has been grinding all day |
| `locked` `stale` | flags, present as a word or absent entirely |

`since` and `age` are different clocks and confusing them makes both useless. An
agent that has started and finished six times is still working on one task, and
`age` is the number that says so.

A token that is not known is *absent*, not empty, so `"{project} {worktree}
{branch}"` renders `dirk main` rather than `dirk  main`. A template naming a
token that does not exist is reported at startup rather than rendering as
silence.

## Naming, in detail

Ported from [namesync](https://github.com/oddurs/namesync), which did this as a
herdr plugin over a socket. Inside dirk the title arrives from the pane's own
OSC callback and the label is a struct field, so the plugin's daemon, client,
state store and sinks all disappear. The policy is unchanged:

| Rule | Behaviour |
| --- | --- |
| Hand-written names win | If a label is not the one dirk last wrote, a human wrote it. The hold is recorded, marked in the nav, and released with `u`. |
| Clearing a name hands it back | An empty label is not a name — it is the clearest statement that the last one was unwanted, so it releases the hold rather than freezing the workspace blank. |
| Defaults are adoptable | `w3`, `tab 2`, the bare repo name — nobody chose these, so they get claimed. |
| Rewordings are not new intent | "naming plugin" → "naming plugins" scores 1.0 on stemmed token overlap and is skipped. |
| Settle before committing | Titles churn early in a turn; a rename waits for the intent to hold still. |
| One rename per workspace per interval | So a fast session cannot strobe the sidebar. |
| Junk is never a name | Shell prompts, bare paths, echoed commands, the plain repo name, and anything in `ignore_titles` are rejected. |
| No guessing across agents | A workspace holding two panes has no single intent. |

### The second source

Naming reads the agent's own terminal title. That is the right primary source —
the work is already done, it costs nothing, and it needs no key — and it fails in
exactly one way: a pane where nothing publishes a title has no intent at all,
and its workspace keeps its project name for ever.

`[naming.sources.llm]` covers that case, and is off unless you ask for it. When
it is on, dirk sends the tail of such a pane's screen and asks for a phrase.

**The model is a source, not a decider.** The phrase it returns is a *candidate
intent*, and it then goes through precisely the policy every title goes through:
junk rejection, the hand-written-name lock, the debounce, the similarity check,
the rate limit. Nothing about *when* a name changes moves into the model.

That is what lets this exist without contradicting the argument the project
started from. The claim was never that models are bad at naming — it was that
generating a name is the part already solved, and deciding when to use one is
the part that is not.

It is asked only about panes with no title, no more often than
`interval_ms`, and never on the drawing thread. Every failure — no key, no
`curl`, a timeout, a malformed answer — leaves naming exactly where it was
without it. Your key is read from the environment; the configuration file holds
only the *name* of the variable, because a configuration file is a thing people
paste into issues.

## Sessions

`dirk` attaches to a session, starting it if it is not running. The session is a
separate process that owns the panes, so closing the terminal does not close
anything in it — come back with `dirk` and everything is where you left it,
including whatever happened while nobody was watching.

```console
$ dirk                      # attach, starting the session if needed
$ dirk --session review     # a session of its own
$ dirk --no-session         # one process, ends with this terminal
```

**The server renders; the client paints bytes it does not read.** That is the
decision the rest follows from. Shipping session state and letting each client
compose it sounds more principled and costs more: two renderers drift, the local
one gets a fix, the remote one does not, and the difference is a rendering bug
nobody can reproduce. There is one renderer, it lives in the session, and a
client is a terminal with a socket — which is also why attaching over `ssh` will
be a small change rather than a second implementation.

One client at a time. A second `dirk` takes the session over and the first is
told why.

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

## Where this is going

The sidebar is the product, and v0.1 has a sketch of it. The roadmap is
[ROADMAP.md](ROADMAP.md), tracked with [cairn](https://github.com/oddurs/cairn);
`cairn next` says what is startable, `cairn show <id>` has the reasoning.

| | | |
| --- | --- | --- |
| **v0.1** ✓ | It runs | panes on a pty, a clickable sidebar, a rail, naming |
| **v0.2** ✓ | The nav | three sections — layouts, spaces, agents — two-line rows carrying worktree, branch, intent and age, and static layouts with a split tree under them |
| **v0.3** ✓ | It knows what the agents are doing | real detection and real lifecycle states, so `blocked` is shown rather than guessed; attention routing and notifications |
| **v0.4** | Sessions that outlive their terminal | a daemon, detach and reattach, persistence, and a socket API with a CLI so an agent inside a pane can drive dirk |
| **v0.5** | A multiplexer you would not miss tmux from | scrollback, copy mode, search, tabs, zoom, a command palette, configurable keys |
| **v1.0** | Production | documented, packaged, hardened, and measured |

The milestones are a dependency order rather than a wish list. Layouts need a
split tree; the API needs a daemon; ordering agents by attention is a re-sort of
a guess until the states are real.

## Status

v0.4 has begun: sessions outlive the terminal they were started from.

v0.3 is done. dirk reads what is running in each pane from its foreground
process group, so a shell is a shell and an agent is an agent; the four
lifecycle states are real, including `blocked`, which is the only one waiting on
a human; the rail counts what is owed and notifications arrive when it changes.

It is still a single process. Close the terminal and the work goes with it,
which is the whole of v0.4. Close the terminal and the work goes with it, so
[herdr](https://herdr.dev) stays installed for anything long-running — that is
item `0007`, and the whole of v0.4.

The gap worth naming rather than burying: the state glyph beside a workspace is
inferred from whether its pane has ever published a title, so everything that
has looks like it is working and `blocked` — the one state waiting on you — is
never shown. That is `0030` and `0031`, and the whole of v0.3.
