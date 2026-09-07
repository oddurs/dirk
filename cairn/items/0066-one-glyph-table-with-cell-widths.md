---
id: 66
title: One glyph table, with cell widths
type: feature
status: backlog
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p1
area: chrome
effort: m
---

## Problem
Glyphs are literals scattered between `theme.rs` (`agent_state` returns the
glyph and the style together) and `nav.rs` (`⑂`, `▾`, `▸`, `·`) and `rail.rs`
(`▊`, `▏`, `◆`, `✕`).  There is no way to offer a set, and no way to run dirk
somewhere the current ones do not render.

## A glyph is a width before it is a picture
Nerd Font glyphs are frequently ambiguous- or double-width, and terminals
disagree about which.  The nav's arithmetic is exact: `write_str` takes a width
budget and `elide` counts characters, so a glyph occupying two cells where one
was budgeted shifts every age, branch and badge on that row and the column stops
lining up.

That is not only a Nerd Font problem -- it is latent today.  `elide` counts
`char`s, and a character is not a cell: a workspace named in Japanese, or one
containing an emoji an agent put in its title, already miscounts.  So this is
half a new feature and half a correction to an assumption that was always
approximate.

Which is the argument for doing it early.  Every later row decoration -- state
rollups, board badges, an attention marker -- goes through the same arithmetic,
and doing them first means doing them twice.

## Shape
`src/glyph.rs` holds one table: a name, and for each of the three sets a string
and its cell width.

```
glyph          ascii  unicode  nerd    cells
blocked        !      ■        (nf)    1
done           +      ●        (nf)    1
working        *      ◐        (nf)    1
idle           ·      ○        ○       1
starting       o      ◦        ◦       1
worktree       y      ⑂        (nf)    1
collapsed      >      ▸        ▸       1
expanded       v      ▾        ▾       1
```

`theme.rs::agent_state` keeps the style and loses the glyph.  `write_str` and
`elide` take cells rather than characters, which means a width function -- the
same one the badge in 0071 will need.

`[nav] glyphs = "ascii" | "unicode" | "nerd"`, defaulting to `unicode`, which is
exactly what ships today.  Not auto-detected: detection is a guess about which
font a terminal was configured with, and guessing wrong here does not degrade,
it breaks the layout.

## Acceptance criteria
- [ ] Every glyph in the interface comes from the table
- [ ] Three sets, chosen by `glyphs =`, defaulting to today's appearance exactly
- [ ] Each entry declares its cell width and the renderer respects it
- [ ] `elide` and the column budgets count cells, not characters
- [ ] A test renders the nav at a known width in the ascii set and asserts every
      row is exactly the sidebar width
- [ ] A test with a double-width character in a workspace name does not shift the
      age column
- [ ] README says which font dirk is designed against and what to set
