---
id: 96
title: The state glyphs are a set, and this is not the only one
type: chore
status: backlog
milestone: v1.0
created: 2026-09-07
updated: 2026-09-07
priority: p3
area: chrome
effort: s
---

## Problem
`! * + ·` were chosen when they were literals in the code.  They are a table
now, so an alternative is a few lines rather than a change -- and herdr's
`○ ◐ ✓` reads better at a glance for the same four states: an open circle for
at rest, a half-filled one for working, a tick for finished.

Not obviously better, which is why this is its own item and a low one: the
current set is legible to somebody who has never seen dirk, and a tick for
`done` competes with the check somebody might read as "passed".

## Acceptance criteria
- [ ] A second shipped set, chosen by name
- [ ] Both sets are one cell per mark, and the ascii set still is too
- [ ] The default does not change without a reason written down
