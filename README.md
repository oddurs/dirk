# dirk

[![ci](https://github.com/oddurs/dirk/actions/workflows/ci.yml/badge.svg)](https://github.com/oddurs/dirk/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/oddurs/dirk?label=release&color=blue)](https://github.com/oddurs/dirk/releases/latest)
[![license: GPLv3+](https://img.shields.io/badge/license-GPLv3+-blue.svg)](COPYING)

A terminal multiplexer that knows what its sessions are for.
[**oddurs.github.io/dirk**](https://oddurs.github.io/dirk/)

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
│   * 1 main                  now│  25 ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀   0043 Pane z│
│       Building the mux core    │ CPU ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀   0044 Create│
│ ▸ · 2 feat/packaging ⑂     1d  │                                    │
│       Reading the grid         │                                    │
│   + workspace                  │                                    │
│   n new  ·  o project          │                                    │
│                                │                                    │
│ agents               attention │                                    │
│   * main                   now │                                    │
│       Building the mux core    │                                    │
│   ↵ go  ·  s sort              │                                    │
├────────────────────────────────┴────────────────────────────────────┤
│ ◆ dirk  ▊1 mux core  ▏2 reading      ! 1  + 2   2 spaces · 14:22   ✕ │
└─────────────────────────────────────────────────────────────────────┘
```

tmux gives you panes and asks you to remember what is in them. dirk starts from
the opposite end: the sidebar is the product, and the panes hang off it.

## Install

A binary for linux or macOS, on either architecture, from
[the latest release](https://github.com/oddurs/dirk/releases/latest):

```console
$ tar -xzf dirk-0.4.0-aarch64-apple-darwin.tar.gz
$ sudo install -m 755 dirk-*/dirk /usr/local/bin/dirk
$ dirk
```

Or from source, which needs Rust 1.88 or newer:

```console
$ git clone https://github.com/oddurs/dirk && cd dirk
$ make && sudo make install
```

[INSTALL](INSTALL) has the rest: verifying a download, `PREFIX` and `DESTDIR`,
uninstalling, and what a packager needs. There is no crates.io release — the
name was taken in 2019 by an unrelated tool — but `cargo install --git
https://github.com/oddurs/dirk` works.

## Three ideas, and keeping them apart is the whole design

**The nav is a column ordered by what is likely to need you.** Boards are what
you glance at, spaces are where the work lives, and between them sits what is
interrupting. Attention flows down the column.

The middle zone is the same workspaces as spaces, not a second list of its own
things — spaces answers "what is open, and where" and it answers "what needs
me". And it is only there when something does: it holds blocked and finished
work, in the order you would deal with them, and occupies no rows at all when
there is none.

**The nav lists what is open.** Not what exists —
`~/Code` has ninety directories in it and a list of ninety things is a file
browser. A project appears once it has a workspace, and disappears when its last
one closes. `o` opens something new.

**Boards are not workspaces — except that they are.** A board is a named
arrangement of programs: one system monitor, one dashboard. There is one of
each, they sit at the top of the column, and they are built the first time you
open one rather than all running in the background so that one of them can
occasionally be glanced at. A board that can say something about itself does so
on its own row, which is what makes it an instrument rather than a link.

Structurally a board *is* a workspace — panes, a split tree, a focused pane and
a name is the whole of one — so there is no second code path for them. A
single-program board and a five-pane dashboard differ only in how many leaves
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

`Ctrl-Space p` opens the palette: everything dirk can do, filtered by typing,
with the key beside each one. It is the answer to "how do I", and it takes the
pressure off binding everything — an action nobody has a key for is still one
keystroke and a word away. Boards and spaces are in it too, so it doubles as a
jump. `dirk --keys` prints the same list to a terminal.

| After the prefix | |
| --- | --- |
| `n` | new workspace in this project |
| `o` | open a project |
| `x` | close the focused pane |
| `\|` `v` | split into columns |
| `-` `s` | split into rows |
| `;` | next pane in this workspace |
| `c` | new tab in this space |
| `,` `.` | previous / next tab |
| `&` | close this tab |
| `z` | zoom: one pane fills the space, and back |
| `{` `}` | move this pane past its neighbour |
| <kbd>Tab</kbd> `j` `k` | next / previous workspace |
| `1` `2` `3` | jump to a page |
| `b` | hide the nav |
| `u` | release a held name, so naming may claim the workspace again |
| `r` | restart a stopped pane |
| `w` | give the nav the keyboard — `j` `k` to move, Enter to go, Escape back |
| `a` | start an agent here |
| `W` | a worktree, and a space open in it |
| `p` | the palette — everything dirk can do |
| `[` | read this pane's scrollback, and copy out of it |
| `/` | find a line, in any pane |
| `d` | **detach** — leave, and let everything keep running |
| `q` | **quit** — end every shell and every agent |

**Leaving and quitting are different things.** A session outlives the terminal it
was started in, so detaching is the ordinary way out: press `d`, or click
`detach` in the bar, and come back to the same panes with `dirk`. Quitting ends
the work. The bar carries both in words for that reason — a single `✕` cannot
say which one it is — and `✕ quit` takes two clicks, because it sits at the edge
of the screen where a stray click is most likely.

<kbd>Esc</kbd> never leaves. It closes the picker, or hands the keyboard back
from the nav, and does nothing else.

Everything else goes straight through to the program in the pane.

## What the column says

Three zones down the left, in the order you ask the questions: **boards** are
what you glance at, **needs you** is what is interrupting, and **spaces** is
where the work lives.

The middle one is only there when something is in it. A heading with nothing
under it is slower to read than no heading, and an empty middle is the fastest
possible way to say that nothing is waiting — the same reason the bar declines
to draw a pair of zeroes. Set `attention = "always"` if you would rather it held
its place, or `"never"` if you would rather read the tree.

| | |
| --- | --- |
| `!` | blocked — waiting on you, and the only state that is owed something |
| `+` | done — finished, and not looked at since |
| `*` | working — producing output |
| `·` | idle — an agent, at rest |
| | nothing is happening here |

A workspace holding a shell at a prompt draws nothing in that column, because
nothing is happening in it. That is also what tells an agent sitting idle apart
from an empty shell, now that they are not listed separately.

A folded project carries the worst state inside it and a count, so twenty repos
fit on a screen and the one that is blocked still says so. `rows = "short"`
drops the branch line, which is scenery — the worktree mark stays, because that
is what tells two rows wearing one repository's name apart.

## Tabs

A space is one piece of work. A tab is one arrangement of programs for it, and
there is usually more than one — an editor and a test runner are the same task
and not the same screen.

The space keeps its name, its agent's state and its clocks, because those are
about the task and a task does not have two of them. The tab keeps the panes.

An unnamed tab is drawn as its number, because a tab called "2" is not
information, and takes a name from whatever is running in it — what is
*running*, not what it says it is doing: an agent's intent is the space's
business, and repeating it on the tab would say the same thing twice in two
columns. `tab rename` overrides that.

The last tab is the space. Closing it is closing that, which `x` on the last
pane already means, so `&` refuses — two ways to do one thing, one of which
leaves a row that draws nothing, is the version worth refusing.

**Zoom is a view, not a change.** The tree is untouched, so leaving puts every
pane back exactly where it was and nothing running notices anything beyond a
resize — the alternative, closing the others and reopening them, is a different
operation and the programs would not survive it. A zoomed space is marked in the
nav, since one pane looks like one pane.

Moving is the same idea: two panes exchange places and neither is restarted.

## Worktrees

Running several agents on one repository means several checkouts, and making one
was: leave dirk, `git worktree add`, come back, open the project. `W` asks for a
branch and does all four — which matters because it is the thing you do to start
*each* agent, not once.

```console
$ dirk worktree list                    # and which one you are in
$ dirk worktree add feat/packaging
$ dirk worktree remove feat/packaging   # --force if it holds uncommitted work
```

The new checkout goes **beside** the repository — `dirk` and `dirk-feat-packaging`
— which is what people do by hand: inside it would be a directory git has to be
told to ignore, and somewhere central would be one nobody finds again. It goes
beside the repository proper rather than beside whichever worktree you are
standing in, or they nest.

Removing one refuses while it holds work nobody has committed. That is git's own
refusal, passed along in git's own words rather than reworded: the case for
removing it anyway is one only you can make, and `--force` is how you make it.

A worktree is marked `⑂` in the nav, which is what tells two rows wearing the
same repository's name apart.

## Reading what has gone past

The wheel scrolls the pane under it, and `Ctrl-Space [` reads the focused one
from the keyboard. Neither touches the program inside — scrolling moves a window
over a grid vt100 was already keeping, so a build does not learn that you looked
at it.

| | |
| --- | --- |
| wheel, <kbd>PgUp</kbd> <kbd>PgDn</kbd>, `g` `G` | move through the scrollback |
| `h` `j` `k` `l`, arrows, `0` `$` | move the cursor; up at the top scrolls |
| `v` | start selecting from here |
| `y` <kbd>Enter</kbd> | copy, and leave |
| `q` <kbd>Esc</kbd> | leave, back at the bottom |

Dragging with the pointer selects and copies in one gesture, which is what
everybody already does. A pane whose program asked for mouse events keeps them —
`less` and `nvim` do their own scrolling and their own selection, and taking
either off them would be worse than not having this.

The bar says how far back you are, because a pane being read from the past looks
exactly like a program that has stopped. Typing brings you back to the live
screen: sending a keystroke to a program whose output you cannot see is the kind
of thing you find out about afterwards.

**Copying happens where you are.** dirk asks the terminal to hold the text
(OSC 52), which is the half that crosses `ssh` — the clipboard is on the machine
you are sitting at, not the one the session is on — and also runs a command on
the client, `pbcopy` or `wl-copy` or `xclip`, since not every terminal answers
the escape. `clipboard = [...]` names your own.

## Finding it

`Ctrl-Space /` searches **every pane in the session**, not just the one in front
of you — the line you are looking for is usually in the pane you were not
watching, which is why each result says where it came from.

Everything the panes have said is read once, when the search opens, and each
keystroke filters what was read. Asking the panes again per character would mean
walking vt100's window over five thousand lines per pane on the thread that
draws; and a search that shifted under you as a build printed is one you could
not read the results of.

<kbd>↑</kbd> <kbd>↓</kbd> move through the matches, <kbd>Enter</kbd> stays where
you landed, and <kbd>Esc</kbd> puts you back where you were — a search you
abandoned should cost you nothing, including your place. The match is marked
with the same highlight a selection uses, so it is ready to copy.

### Boards

A board is a named arrangement you jump to — a dashboard, lazygit, a log tail.
What makes it an instrument rather than a link is that it can report without
being opened:

```toml
[[board]]
name    = "git"
key     = "g"
command = ["lazygit"]
keep    = false              # not left running when you look away
status  = { run = ["dirk-git-badge"], every = "10s" }
```

The first line of what `run` prints becomes a badge on the row, at most eight
columns. dirk does not parse it — the moment it starts understanding git's
output it owns that format for ever, so if you want `3↑ 2•` you write the script
that prints `3↑ 2•`.

No `status` means no subprocess, which is what keeps the default configuration
exactly as cheap as it was. The interval is floored at two seconds, a command
that fails shows `—` and backs off rather than retrying, and none of it runs on
the drawing thread. `keep = false` shuts the board's panes when you look away,
for something you opened for ten seconds that should not sit there holding a
lock.

`[[layout]]` still parses — it is what these were called first.

### Knowing an agent is done

Four states, and the one that matters is `done` — finished work nobody has
looked at. Focusing the workspace is what marks it seen; asking about it over
the socket is not.

dirk works them out from four signals, ranked, and a weak one is never allowed
to make a strong claim:

| | signal | may claim |
| --- | --- | --- |
| 1 | **the agent says so** — a hook runs `dirk agent state done --current` | anything |
| 2 | the terminal — the window title | anything |
| 3 | the screen — a menu of two or more numbered answers | blocked, working |
| 4 | silence — nothing produced for a while, having produced something | done only |

A wrong `working` costs nothing. A wrong `blocked` is an interruption you did
not need, and two of those is how a feature gets turned off — so silence may
promote *working → done* and may never say *blocked*.

**Rank one is worth installing.** `dirk agent hooks claude` prints a snippet;
everything below it is dirk guessing at something the agent already knows.

```console
$ dirk agent hooks claude          # what to paste, and where
$ dirk agent list                  # state, and which signal decided it
```

A report is a claim about a moment, not a lease. It stands until the next one,
or until the pane produces output — because a pane producing text is not
finished, whatever it said a minute ago. Without that rule a harness whose hook
fires on stop but not on start sticks on `done` while it grinds.

### Being told

Two events are worth hearing — an agent blocking, and an agent finishing — and
they have to be distinguishable with your back to the screen, because that is
the point. One interrupts and one satisfies.

Everything else is a rule about when to stay quiet, and those decide whether
this is a feature or something you mute in a week:

- **Nothing about the workspace you are looking at.** If the thing that just
  finished is the one on your screen, you know.
- **Not more often than `notify.min_interval_ms`.** An agent that blocks,
  unblocks and blocks again inside a minute is one interruption.
- **Not at all for a project that asked to be quiet.** The repository you are
  babysitting should be able to be silent without silencing the one you are not.

```toml
[sound]
enabled = true               # off unless asked for; it is an interruption
blocked = []                 # empty takes a sound the system already has
done    = []
```

On macOS an empty command finds two sounds that ship with the machine, so this
works with no file to hunt for. Elsewhere it falls through to the terminal's own
bell — twice for waiting, once for finished, which is as much distinction as a
bell can carry and enough to tell them apart without looking.

**The noise is made where you are.** The session decides *whether* — it holds
the state machine, the seen rule and the floor — and the end with the speakers
decides *how*, from its own configuration. So `dirk --remote build-box` rings
the laptop you are sitting at rather than a machine in a rack. (Desktop
notifications now travel the same way; they used to be delivered by the
session, which was the same bug with a different output device.)

### Starting one

`a` starts an agent where you are looking, making a workspace for it if the pane
you are in is busy. The workspace exists in order to hold an agent — it is why
naming is built around what the agent says it is doing — so it is one keystroke
rather than a new workspace and then a command typed into it.

Which agent comes from the project, then `default_agent`. A Rust repository and
a TypeScript one can want different tools, and being asked twelve times a day is
how a shortcut stops being one.

### Which harnesses

claude, codex, opencode, aider, goose, amp, cursor-agent and gemini ship
recognised. The field adds one a month, so the table is configuration:

```toml
[[agent]]
name    = "sculptor"
argv    = ["sculptor/cli"]        # only consulted for interpreters like node
blocked = { menu = true, match = ["Proceed?"] }
```

A block whose `name` matches a shipped one replaces it whole rather than
merging — somebody overriding claude's markers does not want to inherit half of
ours. Markers are substrings, not patterns: this runs against the screen on
every tick, and a regular expression out of a config file is a way to make a
redraw depend on somebody else's backtracking.

### Keys

Every key is a name and a default, and `[keys]` moves one:

```toml
[keys]
"session.quit" = "Q"
"nav.toggle"   = "H"
```

Rebinding takes the old key away — an action reachable from two keys, one of
which you did not ask for, is how a rebind looks like it did not take. A name
that does not exist is reported at startup rather than ignored, and so is an
action left with no key because something else took it.

### Marks and the font

dirk cannot choose your font; your terminal does. What it can do is not assume
one. `glyphs = "ascii"` draws the whole interface in ASCII, for a terminal or a
font that cannot manage `▾ ⑂ ▊ ✕`.

There is deliberately no Nerd Font set. Those glyphs live in the Private Use
Area, where Unicode assigns no width and terminals disagree — and the nav's
column arithmetic is exact, so disagreeing about width does not look slightly
wrong, it shifts every column after it. `!` and `+` are also simply better than
an icon at one cell: they are legible to someone who has not been taught them.

If you have the font and want them anyway, say so per mark, with the width you
know your terminal gives it:

```toml
[[nav.glyph]]
name  = "blocked"
text  = "\uf071"
cells = 1
```

## Configuration

dirk runs with no config file. `~/.config/dirk/config.toml` overrides what it
names and leaves the rest alone.

```toml
projects_root = "~/Code"     # where `o` looks
sidebar_width = 34
scrollback    = 5000
shell         = ""           # empty means $SHELL

[brand]
mark = ""                    # empty takes the glyph set's; naming one makes it yours
name = "dirk"

default_agent = "claude"     # what `a` starts, when a project does not say
clipboard     = []           # empty finds pbcopy, wl-copy or xclip

[nav]
glyphs    = "unicode"        # unicode | ascii
attention = "when-needed"    # when-needed | always | never
rows      = "tall"           # tall gives a workspace its branch on a second line

[[project]]
path  = "~/Code/dirk"
agent = "claude"             # twelve repositories do not want one answer
sound = true

# A board whose programs are not all on PATH is dropped at startup: an entry
# that could only ever show `command not found` is worse than no entry.
[[board]]
name    = "ptop"
command = ["ptop"]
key     = "1"

# Panes nest. `size` is lines or columns ("5"), a share ("30%"), or absent to
# take an even part of what is left. A pane runs in the directory of whatever
# was focused when the layout opened, unless it names a `cwd` of its own.
[[board]]
name  = "Overview"
key   = "4"
split = "rows"

  [[board.pane]]
  title   = "brief"
  command = ["smali", "brief"]
  size    = "6"

  [[board.pane]]
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
client is a terminal with a socket — which is why attaching over `ssh` is a
transport and not a second implementation.

**Several clients at once, each looking where it is looking.** That is the case
two of them exist for: a laptop and a monitor showing different parts of the
same work. Each carries its own focus, its own nav selection and its own size,
so a key acts on what the person who pressed it can see.

A pane is as wide as the narrowest screen showing it, which is what tmux does
and the only answer that is not a lie to somebody. Detaching one client does not
disturb another. `done` means *nobody* has looked at it, so one viewer clears it
for everybody — which is right, because the work has been seen.

A command over the socket has no screen of its own, so `workspace focus` moves
every client's: which one it would otherwise pick is an accident of ordering.

**A session on another machine is the same session.** `dirk --remote host`
runs `ssh -T host dirk relay` and drives the ordinary client over the pipe pair
ssh hands back — no pty on the far side, because both ends speak a
length-prefixed binary protocol and a line discipline in the middle rewrites
it. Nothing local is started, nothing is rendered twice, and the far side is a
program that copies bytes between a socket and its own stdin and stdout. That
is what "attaching over ssh is a transport change" meant.

```console
$ dirk --remote build-box --session api
```

ssh is the whole authentication story; dirk has no transport of its own and no
business inventing one. The first attempt gets the terminal, so a host key to
confirm or a passphrase to type is asked for where you can see it and answer
it. `DIRK_SSH` names the program that gets you there when a wrapper does, and
`DIRK_REMOTE` the far-side dirk when it is not on `PATH`. `--remote` attaches
and does not carry a command: `ssh host dirk pane list` already works and means
what it says.

A dropped link is not a lost session. The client keeps the terminal, says so at
the top of the screen, and spends two minutes trying to make another, waiting
longer between each — a closed lid is measured in minutes, and the session was
never in the link. A link that comes back and dies again without ever painting
does not reset that patience, which is what stops a host that has gone for good
from being dialled once a second for ever. Ctrl-C leaves while it waits.

Keys pressed while there is nowhere to send them are dropped rather than
replayed: they belonged to the screen that was there when you pressed them.

Local keybindings win, which is what you want when the ssh is running inside a
pane: the outer dirk sees `Ctrl-Space` first, and pressing it twice sends a
literal one through to whatever is inside — including another dirk.

**What comes back is what you arranged, not what was on screen.** A session
writes down its projects, its workspaces and their names, and which projects
were open; it does not write down pane contents. A screenful of text with no
process behind it is worse than an empty pane, because it looks like something
you can type into. Names you wrote by hand come back as yours — still held, so
naming leaves them alone — and a project whose directory has gone is dropped
rather than restored as a row that cannot open anything.

The shape is written on a tick and again on the way out, because ending a
session is exactly when the last second has not elapsed. A session that ended
because its last pane exited writes nothing: that is you closing things, and
recording the emptiness would throw the arrangement away rather than save one.

## Asking a session things

A session answers for itself, which is the difference between a multiplexer
agents happen to run in and one they can work in.

```console
$ dirk pane list
$ dirk pane split w7:p12 rows
$ dirk pane send-keys --current "cargo test"
$ dirk agent list
$ dirk session commands          # the whole surface
```

Answers are JSON, including the failures — a caller is a program, and prose on
stderr is not something a program can branch on.

**Ids are opaque and stable.** `w7` is a workspace and `w7:p12` a pane in it; the
number the nav shows beside a workspace is positional and changes when spaces
are reordered. Every managed pane gets `DIRK_PANE_ID` and `DIRK_SESSION`, and
`--current` resolves from them — so a command from inside a pane reaches the
session holding it without the caller looking anything up first.

`dirk --skill` prints what an agent needs to know to drive a session — generated
from the command table, so it cannot describe a surface that no longer exists.
Most of its value is the traps: each one is something that would otherwise be
found out by getting it wrong.

```console
$ dirk session list          # every session, and whether anyone is watching
$ dirk session reload        # re-read config.toml without restarting
$ dirk session quit          # end one without going and standing in it
$ dirk session prune         # remove sockets nothing is listening on
```

A reload keeps what is running. An open layout keeps its panes — rebuilding a
dashboard because a colour changed elsewhere in the file is not a reload, it is
a restart — and a file that does not parse is reported with the running
configuration kept, because a typo should not cost you the session you were
working in.

**Reads do not mark an agent seen.** Focusing a workspace is what says you have
looked at it; asking about one over a socket is not looking. Without that rule a
status line polling the session would quietly clear every notification it was
built to show.

## Build

```console
$ cargo build --release        # ~6s, 1.0 MB
$ cargo test                   # 16 tests, under a second
$ ./target/release/dirk
```

The unit tests cover the naming policy, which is pure. The rest is a terminal
talking to a terminal, so `tests/smoke.rs` gives dirk a real pty, reads what it
paints and drives it with real keystrokes.

`make install` puts the binary in `$PREFIX/bin`, the manual page in
`$PREFIX/share/man/man1`, and the documentation in `$PREFIX/share/doc/dirk`.
`make uninstall` takes all three back out. `make dist` makes a source tarball
someone else could build from, ChangeLog included.

## The website

[oddurs.github.io/dirk](https://oddurs.github.io/dirk/) is built by
`site/`, a workspace member, and deployed from the repository on every push
that touches it.

```console
$ make site        # build into site/dist
$ make site-serve  # serve it, and rebuild when anything it reads changes
$ make shots       # regenerate the terminal renders from the real binary
```

It is a cargo target rather than an off-the-shelf generator because half of
what belongs on the site is generated from the program. The palette is parsed
out of `src/theme.rs`, so a colour changed there changes on the page and there
is nowhere else for it to be. The roadmap comes from `ROADMAP.md`, the
changelog from `NEWS`, and the screens are real output from the real binary on
a real pseudo-terminal — text rather than an image, so you can select them.

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
on, and each item carries the reasoning that produced it — the problem, the
proposal, what was weighed, and acceptance criteria you can check yourself.

```console
$ make setup                  # once: the git alias and the hooks
$ git work start 44           # claim the item, branch, open a worktree
$ make check                  # fmt, clippy and the suite — the gate CI enforces
$ make shot                   # print what dirk currently paints, as plain text
$ git work ship               # push, pull request, merge itself when green
```

Every branch gets a worktree of its own, so two people — or two agents — can
work at once without sharing a build directory. [HACKING](HACKING) has the whole
loop, [CONTRIBUTING.md](CONTRIBUTING.md) has what a good change looks like,
[AGENTS.md](AGENTS.md) is the brief a coding agent reads, and [NEWS](NEWS) is
what has changed.

## Licence

dirk is free software: you can redistribute it and/or modify it under the terms
of the GNU General Public License as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version. See
[COPYING](COPYING).

It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE.

Contributors keep the copyright in what they wrote and are listed in
[AUTHORS](AUTHORS); what this rests on is in [THANKS](THANKS).

## Where this is going

The sidebar is the product, and v0.1 has a sketch of it. The roadmap is
[ROADMAP.md](ROADMAP.md), tracked with [cairn](https://github.com/oddurs/cairn);
`cairn next` says what is startable, `cairn show <id>` has the reasoning.

| | | |
| --- | --- | --- |
| **v0.1** ✓ | It runs | panes on a pty, a clickable sidebar, a rail, naming |
| **v0.2** ✓ | The nav | three sections — layouts, spaces, agents — two-line rows carrying worktree, branch, intent and age, and static layouts with a split tree under them |
| **v0.3** ✓ | It knows what the agents are doing | real detection and real lifecycle states, so `blocked` is shown rather than guessed; attention routing and notifications |
| **v0.4** ✓ | Sessions that outlive their terminal | a daemon, detach and reattach, persistence, and a socket API with a CLI so an agent inside a pane can drive dirk |
| **v0.5** | A multiplexer you would not miss tmux from | scrollback, copy mode, search, tabs, zoom, a command palette, configurable keys |
| **v1.0** | Production | documented, packaged, hardened, and measured |

The milestones are a dependency order rather than a wish list. Layouts need a
split tree; the API needs a daemon; ordering agents by attention is a re-sort of
a guess until the states are real.

## Status

**v0.4 has shipped.** A session outlives the terminal it was started from:
`dirk` attaches and starts the session if it is not running, close the terminal
and everything in it keeps going, and `dirk session list` says which sessions
exist and whether anyone is watching. There is a socket API and a CLI that
speaks it, so an agent inside a pane can drive the session it is running in.

Before that, v0.3 made the agent states real — dirk reads what is running in
each pane from its foreground process group, so a shell is a shell and an agent
is an agent, and `blocked`, the only state waiting on a human, is shown rather
than guessed.

**v0.5 is what is being worked on now**, and it is the unglamorous half — the
one that decides whether this is usable all day. Scrollback, copy mode and
search have landed; pane zoom, move and swap are in progress.

The gaps worth naming rather than burying: there are no tabs, so a workspace is
one arrangement of panes and not several (`0041`); there is no command palette,
so everything reachable is reachable by a key you have to know (`0042`);
keybindings are not configurable (`0014`); worktrees cannot be made from the nav
(`0044`); and one client watches a session at a time — a second dirk takes it
over and the first is told why (`0060`). Each is in [ROADMAP.md](ROADMAP.md)
with the reasoning attached.
