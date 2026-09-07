---
id: 71
title: Boards that report without being opened
type: feature
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: layout
effort: l
---

## Problem
A layout is a link: a name you press to go somewhere.  A dashboard you have to
open in order to find out whether it matters is a link with extra steps.

## One field turns a link into an instrument
A status command whose output becomes a badge on the row.  That is the whole
difference between "a place called git" and "the tree is dirty and two commits
behind"; the same for ptop, for a log tail, for cairn's p0 count.

```
 g  git            3↑ 2•
 p  ptop              47%
 l  logs               2!
 c  cairn           4 p0
```

The badge is advisory.  It never changes what pressing the row does, it never
blocks a frame, and a board without one is exactly as cheap as a layout is
today.

## Cost, said plainly
This is the only item in the nav concept that adds a recurring subprocess, and
four boards at ten seconds is twenty-four processes a minute for as long as dirk
is running.  That is a real cost and it is why the answer is: no `status` in the
config, no subprocess, and the shipped defaults have none.

## Shape
* `[[board]]` as the config key, with `[[layout]]` kept as a serde alias so
  existing files keep working and the docs only teach one word.
* `status = { run = [...], every = "10s" }`, floored at two seconds.
* One scheduling thread for all boards, results arriving as an `Ev` -- the same
  shape as the existing sampling loop, and never on the drawing thread.
* Output is taken as one line, elided to eight cells using the width function
  from 0066.  A badge that wraps has already lost the argument.
* Failure shows `—` and backs off: skip five intervals, doubling to a cap.  A
  command that cannot run is a configuration problem, and retrying it every two
  seconds turns one mistake into a fan.
* `keep = true|false` decides whether the board's panes stay running when you
  look away.  A monitoring dashboard keeps; lazygit does not.  One boolean, not
  a second concept -- a separate "utility window" type would be two code paths
  that drift.
* Boards remain singletons.  Opening one twice goes to the one already running,
  which `merge_layouts` already assumes.

## Not doing
Parsing the output.  A badge is a string the board's own command produced, and
the moment dirk starts understanding git or cairn it owns their formats for
ever.  If you want `3↑ 2•` you write the script that prints `3↑ 2•`.

## Acceptance criteria
- [ ] `status = { run = [...], every = "10s" }` on a board
- [ ] Rendered as at most eight cells on the row, one line, elided
- [ ] Never on the drawing thread; the interval is floored and the floor is reported
- [ ] A failing command shows a dash and backs off rather than retrying
- [ ] `keep` decides whether the panes survive looking away
- [ ] No `status` in the config means no subprocess, ever
- [ ] `[[layout]]` still parses, and the README teaches `[[board]]`
