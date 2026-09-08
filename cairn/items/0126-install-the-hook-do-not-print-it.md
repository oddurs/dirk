---
id: 126
title: Install the hook, do not print it
type: feature
status: backlog
milestone: v0.7
created: 2026-09-07
updated: 2026-09-07
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
- [ ] `install`, `uninstall`, `status`; bare `hooks` still prints
- [ ] Edits between markers; the rest of the file is byte-identical
- [ ] A hand-merged snippet is detected and reported, not overwritten
- [ ] `status` reports version and drift per harness
- [ ] Installing twice is a no-op
- [ ] Every shipped harness that has hooks is supported
