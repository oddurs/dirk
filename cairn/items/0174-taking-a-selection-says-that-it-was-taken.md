---
id: 174
title: Taking a selection says that it was taken
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p2
effort: s
area: chrome
---

## Problem
Dragging across a pane selects, and releasing takes the selection -- to the
system clipboard by command and to the terminal by OSC 52, both quietly.
Nothing on screen says it happened.

For the keyboard that was fine: you pressed a key, so you know. For a pointer
it is the most common gesture in the whole application ending in silence, and
the failure mode is somebody dragging again harder because they cannot tell
whether it worked. The clipboard write is also the one part that genuinely can
fail -- there may be no `pbcopy`, the terminal may ignore OSC 52 -- and the
code deliberately says nothing, because "there is nothing useful to say to
somebody who has no clipboard tool installed".

There is something useful to say. Not about which mechanism worked -- about
whether anything did.

## Proposal
A brief acknowledgement in the rail when a selection is taken: how much, and
that it went. It lives where the rail already says transient things, and it
goes on its own after a moment.

The honest version needs one thing the code does not currently keep: whether
either mechanism succeeded. The command is spawned onto a thread and its result
dropped; OSC 52 is written and never confirmed. Confirming the command is
cheap. OSC 52 cannot be confirmed at all, which means the message can only
honestly be "copied" when the command worked, and something weaker when only
the escape sequence went -- or the same message with the knowledge that on
Terminal.app it is a small lie.

That is a real trade-off and it is the interesting part of this item. Silence
is not the answer; a confident "copied" that is sometimes false is worse than a
"copied" that is sometimes vague.

## Acceptance criteria
- [ ] Taking a selection is acknowledged, briefly, in the rail
- [ ] The acknowledgement says how much was taken
- [ ] The clipboard command's success is known rather than discarded
- [ ] The message does not claim more than dirk can know
- [ ] It clears on its own and is never the reason a frame is drawn twice
- [ ] Nothing is said when nothing was taken -- a click is not a selection
