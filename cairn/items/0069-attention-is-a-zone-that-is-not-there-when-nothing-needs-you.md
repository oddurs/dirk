---
id: 69
title: Attention is a zone that is not there when nothing needs you
type: feature
status: done
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: nav
effort: m
---

## Problem
`agents` is a permanent third section duplicating rows already in `spaces`.  The
justification in `nav.rs` is sound -- spaces answers "what is open, and where"
and agents answers "what needs me" -- but the cost is a fixed tax on vertical
space in exchange for information that is usually "nothing", and at forty
workspaces the two lists together do not fit.

## Pull the rows, do not re-sort the tree
When an agent blocks, the useful action is not "re-order my tree".  That costs
you your place in it, and your place in it is what you were doing.  What you
want is the two rows that matter, at the top of the column where your eye
already is, with the tree untouched underneath.

And when nothing needs you the section should occupy no rows at all.  That is
the principle `rail.rs` already applies when it declines to draw a pair of
zeroes: an empty middle is the fastest possible way to say that nothing needs
you, and a heading with nothing under it is slower to read than no heading.

## Shape
`Section::Attention` replaces `Section::Agents`.  The rows are the workspaces in
`blocked` and `done`, ordered blocked before done and oldest first within each,
which is the order you would deal with them in.

That ordering is the only sensible one, so `s sort` goes away and the key is
freed.  The row type already exists (`Row::Agent`), and `nav_rows` already
builds a flat list before rendering, so this is a change in how the list is
assembled rather than in how it is drawn.

`[nav] attention = "always" | "when-needed" | "never"`, and `sections = [...]`
for the order of the three, because the argument for this order is good but it
is still an argument.

When the section is absent, its heading and its footer are absent too -- not
drawn empty, not drawn dim.  Zero rows.

## Acceptance criteria
- [ ] The zone holds exactly the workspaces in blocked and done
- [ ] Blocked before done, oldest first within each
- [ ] It contributes zero rows -- heading and footer included -- when there are none
- [ ] `attention = always | when-needed | never`, and `sections` sets the order
- [ ] Selecting a row goes to the workspace without collapsing or re-ordering the tree
- [ ] A test asserts the row count is unchanged by an idle agent appearing
