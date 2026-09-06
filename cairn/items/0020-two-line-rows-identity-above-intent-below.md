---
id: 20
title: 'Two-line rows: identity above, intent below'
type: feature
status: planned
milestone: v0.2
depends_on:
- 10
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: nav
effort: m
---

## Problem
A workspace row currently shows one line: a state glyph and a label. The label
is either the project name or whatever naming last wrote, so a row can say
'Open source wifi e-reader' with no indication of which repository that is, or
say 'bedreader' with no indication of what is happening in it. One line cannot
carry both, and both are needed.

## Proposal
Two lines per row, as in herdr:

    ▸ · 1 fontina                                            10m
        main Astro docs site GNU style
    ▸ · 2 fontina ⑂                                            1d
        feat/packaging-manifests Richard Stallman perspective

Line one is identity: expander, state glyph, number, project, worktree mark,
and age right-aligned. Line two is context: branch, then intent, dimmer.

The number is the jump key, so it belongs on the line the eye lands on. The
branch belongs with the intent because together they say what this checkout is
for; on the identity line it would compete with the project name.

## Acceptance criteria
- [ ] Identity and intent on separate lines, with the second indented and dim
- [ ] Worktrees marked distinctly from ordinary checkouts
- [ ] Both lines elide independently, at the point they are read
- [ ] Clicking either line selects the workspace
- [ ] A row with no intent yet draws one line, not a blank second one
