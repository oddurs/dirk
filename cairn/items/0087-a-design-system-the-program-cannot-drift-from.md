---
id: 87
title: A design system the program cannot drift from
type: feature
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: m
area: web
---

## Problem

A website for a terminal program that does not look like the terminal program
is a brochure for something else. dirk already has a palette — Gotham, in
`src/theme.rs`, as semantic tokens rather than colours — and the moment the
site writes `#33859d` into a stylesheet, there are two palettes and one of them
is wrong.

## Proposal

The dark palette is generated. `site` parses the Gotham ramp out of
`src/theme.rs` and emits it as custom properties, so changing a colour in the
program changes it on the site and there is no second place to change.

Everything above the palette is the same three rules the chrome already lives
by, restated for a page:

**Colour is a role, never a literal.** The tokens are named for the job:
`--text`, `--dim`, `--faint`, `--accent`, `--ok`, `--warn`, `--critical`,
`--branch`, `--worktree`, `--intent`. They are the method names on `Theme`.

**Chrome is not content.** The header, the nav and the footer paint their own
ground. The reading column paints nothing of its own. That difference is what
makes the navigation read as navigation, on a page as much as in a pane.

**The grid is a character cell.** Spacing derives from the cell width and line
height rather than from a designer's eight-point scale, so the terminal
renders on the page sit on the same rhythm as the prose around them.

The light ramp is authored rather than generated — the program has no light
mode to generate one from — and maps the same roles onto a light ground. It
is documented as authored so nobody goes looking for where it came from.

## Acceptance criteria

- [x] The dark palette is generated from `src/theme.rs` at build time
- [x] No hex literal appears in a hand-written stylesheet
- [x] Light and dark both work, and follow the system unless overridden
- [x] The system covers: type scale, spacing, the reading measure, links, code,
      tables, callouts, the terminal frame, nav, header and footer

## 2026-09-07

The generated palette turned out better than expected: parsing the method
bodies in `Theme` rather than only the constants means the CSS carries the
role names the program uses -- `--branch`, `--worktree`, `--intent`, `--ok` --
and a role naming a constant that has gone is a hard error at build time rather
than an invisible word on a page.

One thing to look at with your eyes before trusting it. The light ramp is
derived from the dark one with relative colour syntax -- `oklch(from
var(--gotham-base8) 0.985 calc(c * 0.35) h)` -- which satisfies the no-literals
rule and has been reasoned about but not seen. A browser too old for relative
colour drops every one of those declarations together and stays dark, which is
a whole design rather than half of two; but the lightness values themselves are
arithmetic, not judgement, and judgement is what they need.
