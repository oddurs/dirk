---
id: 126
title: Install the hook, do not print it
type: feature
status: done
milestone: v0.7
created: 2026-09-07
updated: 2026-09-08
priority: p0
area: agents
effort: m
---

## Problem
`dirk agent hooks claude` prints a snippet and tells you where to paste it. The
README calls rank one "worth installing" and then asks you to install it by
hand, which means most people run on rank two for ever and dirk spends its life
guessing at something the agent already knows.

## Proposal
`dirk agent hooks install claude` writes it. `status` says which harnesses have
it, at which version, and whether the file has drifted. `uninstall` removes
dirk's part and leaves the rest of the file alone.

The care is all in not owning a file dirk did not write. Edit in place inside
delimited markers, never rewrite the document; refuse and explain when the
markers are missing but dirk's keys are present, because that means somebody
merged it by hand and a silent overwrite would lose their work.

`hooks` without a verb keeps printing, for the case where you want to read it
first.

## Acceptance criteria
- [x] `install`, `uninstall`, `status`; bare `hooks` still prints
- [x] Everything that is not dirk's is untouched, key order included — see below
- [x] A hand-merged snippet is detected and reported, not overwritten
- [x] `status` reports version and drift per harness
- [x] Installing twice is a no-op
- [x] Every shipped harness that has hooks is supported

## 2026-09-08

Markers do not work here. The file is `~/.claude/settings.json` and JSON has no
comments, so there is nowhere to leave one — and there is no byte-identical edit
of a JSON document without a parser that reports spans, which serde_json is not.

So dirk recognises its own entries by shape: a command that reports a state to
dirk from inside a pane. That gives install, uninstall and idempotence honestly,
and it gives the hand-merge case for free — an entry that is dirk-shaped and not
dirk's current text is somebody's edit, reported and left alone.

`preserve_order` on serde_json, so the keys somebody else wrote come back in the
order they wrote them. Indentation becomes dirk's, which is the one thing this
cannot preserve and is documented as such. It also changes every JSON answer
from alphabetical to insertion order, which is a small improvement everywhere
and one regenerated `doc/api.json`.

Uninstall removes a group only when every command in it is dirk's. Taking
somebody's hook out because it shares a matcher with ours is the kind of damage
that is noticed weeks later.

"Version and drift" is answered by `installed` / `absent` / `edited` rather than
by a number: what a caller needs to know is whether the file says what dirk
would write now, and a version would be a second thing to keep in step.
