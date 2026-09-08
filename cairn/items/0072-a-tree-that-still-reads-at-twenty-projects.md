---
id: 72
title: A tree that still reads at twenty projects
type: feature
status: done
milestone: v0.5
depends_on:
- 66
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: nav
effort: m
---

## Problem
Two-line rows -- name and age, branch underneath -- are right at five workspaces
and wrong at forty.  And a collapsed project shows nothing about what is inside
it, so collapsing to make things fit means losing the blocked agent you
collapsed past.

Between them, those two make the tree stop working at about six projects, which
is fewer than a person who wants this tool has.

## A collapsed row still has to be worth reading
The rollup is the whole idea: a collapsed project shows a count and the worst
state among its workspaces.

```
 ▸ herdr       ! 3
 ▸ smali         1
 ▸ ptop        + 2
```

Worst is `blocked` > `done` > `working` > `idle`, which is the same order the
attention zone uses and the same order you would deal with them in.  With it,
twenty projects fit a screen and you can still see that one of them wants you.
Without it, collapsing is a way of hiding exactly the thing the sidebar exists
to surface.

## Density
`[nav] rows = "tall" | "short"`.

Tall is today: name and age, branch on a second line.  Short is one line per
workspace and drops the branch -- but **keeps the worktree glyph**, because the
branch is scenery and the fact that this is a worktree at all is not.  That
distinction is already in `nav.rs`, which calls the branch line scenery and
lands selection on the identity line; short simply takes the scenery away.

## Not doing
Collapse-others, filtering, or a search.  Those are their own items and the tree
should be readable before it needs to be searchable.

## Acceptance criteria
- [ ] A collapsed project shows a count and the worst state among its workspaces
- [ ] Worst is blocked > done > working > idle
- [ ] `rows = tall | short`, defaulting to tall
- [ ] Short keeps the worktree mark and drops the branch line
- [ ] Twenty projects with agents fit an 80x24 terminal and stay readable
- [ ] A test asserts a collapsed project reports a blocked workspace inside it
