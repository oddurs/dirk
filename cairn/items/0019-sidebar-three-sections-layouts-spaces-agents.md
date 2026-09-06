---
id: 19
title: 'Sidebar: three sections — layouts, spaces, agents'
type: feature
status: done
milestone: v0.2
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: nav
effort: l
---

## Problem
The sidebar has two sections, pages and projects, and neither is what the nav
should be. `pages` is a list of single programs; the thing above spaces should
be a list of *layouts*, which are named multi-pane arrangements. And there is no
agents section at all, so the question the nav exists to answer — which agent
needs me — is not asked anywhere.

## Proposal
Three sections, top to bottom, each with a letter-spaced heading and a count on
the right:

    L A Y O U T S                    3
      Overview
      Review
      Focus
    S P A C E S                     10
      ...
    A G E N T S            priority
      ...

They are three lists of different things and the ordering is deliberate:
layouts are places you go, spaces are where work lives, agents are what is
asking for you. Attention flows down the column.

The same workspace appears in both spaces and agents. That is not duplication:
spaces answers 'what is open, and where', agents answers 'what needs me', and
they sort differently for that reason.

## Acceptance criteria
- [x] Three sections, each independently collapsible
- [x] A count per section header
- [x] Sections keep their proportions when the terminal is short, rather than
      the last one being cut off entirely
- [x] Every row in all three is a click target

## Plan

The sidebar renders in one pass with a moving `y` cursor, which is why it has no
scrolling and no selection: there is nothing to scroll or select, only a cursor
that has already moved on.

Replace it with a flat `Vec<Row>` built from the session, then render a window
of that. Scrolling becomes an offset, selection becomes an index, and hit
testing becomes the same lookup the renderer already does — three features that
were each awkward alone fall out of one list.

Keyboard and pointer converge on `Target`, the vocabulary the hit map already
speaks: pressing Enter on a row and clicking it produce the same value and go
through the same `act`. Without that they are two implementations of every
action, which is how they drift.

Selection is distinct from focus and needs somewhere to live, so the nav takes
keys directly while it holds them — `ctrl-space w` in, Escape out. A nav that
needs the prefix before every `j` is not a nav.

The agents section lists workspaces whose pane has published an intent, which
is the same crude signal the state glyph uses today. 0021 orders it by what is
owed and 0031 makes the states real; this ships the shape.
