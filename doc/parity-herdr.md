# Reading herdr for roadmap targets

herdr solves the same problem dirk does and has three years of field reports
behind it. This file is how its surface gets turned into dirk items without
turning dirk into it.

Snapshot: `herdrdev/herdr` @ `9e01168b`, 2026-09-08. Re-run the loop at the
bottom against a newer SHA rather than editing this by hand.

## Method

**Read the docs, not the source.** herdr is ~380k lines across Rust and Zig.
Its English documentation is 4,467 lines in 21 files
(`docs/next/website/src/content/docs/*.mdx`) and enumerates the entire
user-visible surface. Source tells you how they built it, which is the question
you ask *after* deciding you want the thing. Docs tell you what they decided was
worth shipping, which is the only question a roadmap asks.

**Four columns, and the fourth is the point.** Every capability gets: what herdr
does, what dirk does, a verdict, and a one-line reason. The verdicts are

| | |
| --- | --- |
| `have` | dirk already does this |
| `partial` | dirk does some of it; the remainder is an item |
| `adopt` | take it, roughly as designed |
| `adapt` | take the problem, not the solution — dirk's answer differs |
| `decline` | deliberately not doing this, and here is why |

`decline` rows are the ones that pay. Without them the same question gets
re-asked every time herdr ships, and the answer drifts toward yes because
nobody wrote down why it was no.

**Only `adopt` and `adapt` become cairn items.** The rest stays here.

## The matrix

### Mux primitives

| herdr | dirk | verdict | note |
| --- | --- | --- | --- |
| Tabs between workspace and pane | — | adopt | already `0041` |
| Zoom, swap, move panes | in progress | have | `0043` |
| Resize mode (`prefix+r`, directional) | drag divider only | adopt | keyboard resize is missing entirely |
| `pane move --new-tab` / `--new-workspace` | — | adopt | falls out of tabs |
| Pane rename (manual label) | naming owns the name | adapt | dirk has locks (`0056`); a pane-level manual label is the same idea one level down |
| `ui.pane_borders` auto/always/off | always split-only | adopt | cheap; one config key |
| Copy mode: `w/b/e`, `W/B/E`, `{}`, `ctrl+u/d` | `hjkl`, `0$`, `gG`, page | adopt | word motions are the gap people feel |
| Copy mode: `/` `?` `n` `N` inside the pane | `/` searches all panes | partial | dirk's cross-pane find is better; in-copy-mode search is still missing |
| Kitty graphics in panes | — | adopt | big, but it is what makes a pane a real terminal |
| `tab_bar_right`: hostname, datetime, command | boards carry `status = { run, every }` | have | same mechanism, aimed at rows instead of a status bar |
| Mouse capture off | always on | adopt | one key; someone will want it |

### Keys and configuration

| herdr | dirk | verdict | note |
| --- | --- | --- | --- |
| Every binding configurable, prefix included | fixed | adopt | already `0014` |
| Prefix-free direct chords, with a vetted `ctrl+alt` family | — | adopt | the *research* is the deliverable — they mapped ten terminals' defaults |
| `[[keys.command]]` — bind a shell command | — | adapt | boards already run commands; a binding that opens a board is the dirk-shaped version |
| `prefix+?` binding help, filterable | — | have | `0042` command palette subsumes it |
| `window_title` with `{hostname} {workspace} {tab}` tokens | — | adopt | dirk emulates panes' titles and writes none of its own |
| `terminal.default_shell`, `shell_mode = login` | `shell = ""` | adopt | `shell_mode` is a real macOS `PATH` bug, not a preference |
| `terminal.new_cwd` = follow/home/current/path | follow | adopt | trivial |
| Sidebar `rows = [[tokens]]` — compose row layouts | `rows = "tall" \| "short"` | adapt | full composition invites unreadable rows; a third named layout is the dirk answer |
| Value-driven token colour rules | — | decline | the nav's job is ranking attention, not letting you re-encode it |
| Custom light/dark theme overrides | `theme.rs` | partial | check whether the light/dark split exists |
| Config reload without restart | | have | `0038` |

### Agents

| herdr | dirk | verdict | note |
| --- | --- | --- | --- |
| Detection rules as TOML manifests, one file per agent | `[[agent]]` blocks in config | adapt | same shape; dirk should split them into `~/.config/dirk/agents/<name>.toml` so one can be replaced without editing config |
| Remote manifest updates fetched from a vendor endpoint | — | decline | a redraw that depends on rules downloaded overnight is not a redraw you can debug |
| `agent explain <target>` — why this state | `agent list` names the deciding signal | partial | dirk has the data; it needs the command, and `--file` for offline replay |
| `agent rename`, custom display labels | naming owns it | have | `0056` locks |
| `integration install/uninstall/status <agent>` | `agent hooks claude` prints a snippet | adopt | printing a snippet is a README; installing it is a feature |
| Native session restore — `claude --resume <id>` after server restart | shell in the saved cwd | adopt | the single highest-value item on this page |
| `agent prompt <target> <text> [--wait]` | — | adopt | dirk can start an agent but cannot talk to one |
| `agent wait --until blocked\|idle\|done` | — | adopt | the primitive that makes dirk scriptable by an agent |
| `pane wait-output --regex` | — | adopt | same, for things that are not agents |
| `pane report-agent` / `report-metadata` from outside | `agent state` | partial | metadata tokens for display are the missing half |
| `HERDR_AGENT=` env hint for sandbox wrappers | — | adopt | ten lines; unblocks anyone running an agent under a jail |
| Alternate-screen history reads (drives the agent's own scroll to read transcript) | — | adopt | clever and non-obvious; full-screen agents keep history where scrollback cannot see it |
| `agent attach` — one pane in your terminal, no UI | — | adopt | pairs with the socket API for scripts |
| Per-agent sound overrides | per-project | adopt | one more key on an existing table |
| 23 supported agent kinds | 8 | adapt | the table is config in both; do not chase the list |

### Server and sessions

| herdr | dirk | verdict | note |
| --- | --- | --- | --- |
| Named sessions | | have | `0037` |
| Per-client view | | have | `0060` |
| Headless terminal size when no client is attached | — | adopt | required before any of the automation items are usable unattended |
| Pane screen history replay after restart, off by default | — | adopt | and keep their default: pane output holds secrets |
| Live handoff — move running ptys to a replacement server | — | adopt | how you ship an update without killing the work |
| `update`, `channel stable\|preview` | releases only | adapt | self-update is right; two channels is a project with a release manager |
| Saved SSH machines, several in one window | `--remote` one at a time | adopt | this is herdr's flagship and it fits the nav exactly |
| Remote binary bootstrap over ssh | — | adopt | `--remote` is a demo without it |
| `terminal session observe` / `control` — JSON frame stream | — | adopt | how a third party builds a client without linking dirk |
| `notification show` from the CLI | — | adopt | cheap |

### Git

| herdr | dirk | verdict | note |
| --- | --- | --- | --- |
| Worktree create / open / remove from the nav | display only | adopt | already `0044` |
| Worktrees grouped under the parent workspace | — | have | `0094` |
| `worktrees.directory` root | — | adopt | noted on `0044` rather than filed |

### Extensibility

| herdr | dirk | verdict | note |
| --- | --- | --- | --- |
| Plugins: manifest, actions, startup hooks, panes, link handlers, storage | — | adapt | "the CLI is the plugin API" is the right instinct and dirk already has the CLI; ship the completions and schema first, defer the manifest |
| Marketplace | — | decline | a distribution problem, not a multiplexer problem |
| `api schema --json` | — | adopt | cheap, and it makes the socket API testable |
| Shell completions for five shells | — | adopt | cheap |
| Agent skill file | `--skill` | have | and dirk's is generated from the command table |

### Platform

| herdr | dirk | verdict | note |
| --- | --- | --- | --- |
| Windows client and server | — | decline | ptys, signals and sockets all differ; it is a second product |
| Japanese and Chinese docs | — | decline | |
| CJK IME cursor tracking, prefix input-source switching | — | decline | downstream of Windows |
| Homebrew / mise / nix | tarball + `make install` | adopt | packaging, not features |

## Proposed milestones

Ordered so that each one is useful before the next starts.

**v0.6 — Driving it from outside.** `agent prompt`, `agent wait`,
`pane wait-output`, `agent attach`, headless terminal size, `api schema`, shell
completions, `notification show`. dirk can already be *asked* things; this is
what lets an agent *use* it. It is also the milestone most aligned with what
dirk claims to be, so it goes first.

**v0.7 — Agents you do not have to babysit.** Detection manifests as files,
`agent explain`, `integration install`, native session restore, a sandbox-wrapper
env hint, alternate-screen reads, per-agent sounds. Native session restore is
the item that changes how the tool feels.

**v0.8 — A multiplexer you would not miss tmux from, part two.** Finishes v0.5:
tabs, keyboard resize, copy-mode word motions and search, pane borders,
`window_title`, shell mode and cwd policy, kitty graphics.

**v0.9 — Coming back to it.** Pane history replay, live handoff, self-update,
`terminal session observe`/`control`.

**v0.10 — More than one machine.** Saved machines in the nav, remote binary
bootstrap. Deliberately last: it is herdr's flagship and dirk's nav is the right
place for it, but it is worth nothing until the four above are solid.

Worktree items stay where they are: `0044` in v0.5, `0094` in v1.0.

Filed as `0111`–`0115` (the milestones) and `0116`–`0149` (34 items).

## The re-run loop

herdr's `Added` sections are a clean feature feed. To diff since this snapshot:

```console
$ cd ~/Code/herdr && git fetch && git log --oneline 9e01168b..origin/main -- docs/
$ git diff 9e01168b..origin/main -- docs/next/CHANGELOG.md | grep '^+- '
$ git diff 9e01168b..origin/main --stat -- docs/next/website/src/content/docs/
```

Then update the SHA at the top, add rows for what is new, and file only the
`adopt` and `adapt` ones. Anything that lands in a `decline` row's territory
gets a sentence appended to that row instead of a new item.
