---
id: 65
title: Detach, and two exits that say which is which
type: bug
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p0
area: nav
effort: s
---

## Problem
`Ctrl-Space q` (`src/main.rs`, the `command` table) is the only way out from
inside dirk, and it ends every shell and every agent in the session.
`Ctrl-Space d` toggles the sidebar.  There is no detach binding at all: to leave
a session running you close the terminal window.

## Leaving is not quitting
Since 0.4 the session outlives its terminal, which is the headline feature of
that release.  So the vocabulary has two words -- detach and quit -- and the
interface has one, bound to the destructive one.

Look at the frequencies.  Detaching is something you do several times a day and
should cost nothing.  Quitting is something you do once, deliberately, and it
destroys work that cannot be recovered.  Today those are inverted: the common
action is unbound and the rare one is a single keystroke.

The rail has the same problem in one character.  A `✕` cannot say whether it
parks your work or ends it, and the rail is the one surface on every screen --
which makes it the right place for the answer to "how do I get out of this",
and the wrong place for an ambiguous glyph.

## Shape
Detach already exists as a mechanism; nothing calls it.  The server handles a
client going away (`Ev::Detach` clears the view), and the client already handles
being told to leave (`wire::Kind::Bye` restores the terminal and prints the
reason).  So `d` is a prefix command that makes the server say goodbye to its
view with the reason `detached`, and the session carries on.

* `d` detaches.  `b` takes over the sidebar toggle -- bar, and toggled far less
  often than you leave.
* `q` is unchanged: still quit, still armed twice, still ends everything.
* `RESERVED` in `config.rs` gains `b`, so a layout cannot be bound to a key that
  will not reach it.
* The rail replaces the lone `✕` with `detach` and `✕ quit`, both clickable, the
  destructive one keeping its arming.  `Target::Detach` is new.
* `Esc` never leaves, from any mode.
* USAGE, the README and `--skill` all say `q` quits; they need the other half.

## Not doing
A confirmation for detach -- it is not destructive and a prompt would make the
cheap action expensive, which is the bug.  And no `Ctrl-D`: that belongs to the
shell in the pane.

## Acceptance criteria
- [ ] `d` detaches; `dirk session list` then shows the session running, not attached
- [ ] Reattaching finds the same panes, still running
- [ ] `b` toggles the sidebar
- [ ] `q` still quits, still armed twice
- [ ] The rail shows both, in words; clicking detach detaches and clicking quit arms
- [ ] `Esc` never leaves, from any mode
- [ ] A session test: attach, detach, reattach, find the same workspace
