---
id: 67
title: Harnesses are configuration, not a table in agent.rs
type: feature
status: done
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: agents
effort: m
---

## Problem
`KINDS` in `agent.rs` hardcodes four harnesses -- claude, codex, aider, goose --
recognised by process name and argv substring, and judged blocked by matching
prose like `"Would you like"`.  The field adds one a month: opencode, amp,
cursor-agent, whatever is next.  Every addition is a Rust change and a release,
for what is a row in a table.

## The catalogue and the policy are different things
`agent.rs` currently holds both.  The catalogue is what claude *looks like* --
its name, its argv, the words it uses when it wants an answer.  The policy is
what counts as blocked at all: the rule that a marker only means blocked when
there is also a menu of two or more numbered choices, which exists because every
finished turn ends in a question and without it every completed agent read as
waiting.

The catalogue changes monthly and belongs in a file.  The policy changes rarely,
is the part that was hard to get right, and belongs in code.  Splitting them is
the whole ticket, and it is the same move the naming policy made when tokens and
templates left `name.rs`.

## Shape
```toml
[[agent]]
name    = "opencode"
names   = ["opencode"]              # process names
argv    = ["opencode/cli"]          # argv substrings
blocked = { menu = true, match = ["Approve", "Allow command?"] }
```

The four current kinds ship as defaults expressed in exactly that struct, so the
schema is proven by the things already using it.  A user's `[[agent]]` with a
name that matches a default **replaces** it wholesale rather than merging field
by field -- predictable beats clever, and someone overriding claude's markers
does not want to inherit half of ours.

Matching stays substring, not regex.  `is_blocked` runs against the prompt
window on every tick for every agent pane; a regex over a few hundred columns,
supplied by a config file, is a way to make a redraw depend on somebody's
backtracking.  The menu rule is what carries the precision anyway.

Validation goes into `config::complaints`, which 0.4 added for exactly this: an
`[[agent]]` with neither `names` nor `argv` can never match anything, and saying
so at startup is cheaper than wondering why a harness is never recognised.

## Acceptance criteria
- [ ] `[[agent]]` blocks define a harness; the four current kinds ship as defaults
- [ ] A user block replaces a default of the same name wholesale, and that is documented
- [ ] A user-defined harness is recognised, named and stated like a built-in one
- [ ] An unmatchable definition is reported at startup and on `session reload`
- [ ] `agent.rs` keeps `is_blocked`, `identify` and the menu rule, and loses the catalogue
- [ ] `agent list` reports which kind matched, so a caller can tell what it is looking at
