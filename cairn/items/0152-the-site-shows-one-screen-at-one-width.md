---
id: 152
title: The site shows one screen, at one width
type: feature
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: m
area: web
---

## Problem

dirk's argument is that the sidebar is the product and that the chrome gives
things up in an order as the terminal narrows. The site shows one render, of
the whole screen, at ninety-two columns — where the nav is a third of a small
image and the bar's hierarchy is four pixels tall.

It also never shows the states that are the whole point: an agent blocked, the
attention zone holding something, a chord open.

## Proposal

`DIRK_SHOT_SIZE` exists now, so `make shots` can produce more than one:

- the nav on its own, wide enough to read
- the rail at three widths, which is the ladder, visible
- a session with something owed

Each is real output from the real binary, a few kilobytes of text. The front
page can then make its argument with evidence instead of three cards of prose.

## Acceptance criteria

- [x] `make shots` produces every render the site uses, reproducibly
- [x] The front page shows the nav at a size where it can be read
- [x] The ladder is shown rather than described
- [ ] A render is legible on a phone, or is scrollable and says so

## 2026-09-07

Four renders instead of one. `DIRK_SHOT_ROWS` emits a band, so the bar at three
widths is three rows of about a kilobyte each rather than three whole screens
of unchanged scrollback around them — and consecutive terminals join into one
stack in the stylesheet, because it is one thing being shown three times.

The ladder is now shown on the front page rather than described.

Not done: a render of a session with something owed. Staging a blocked agent in
the shot driver is a fake agent and a prompt to answer, which is a test harness
rather than a screenshot, and it belongs with the tests that already do it.

The last criterion stays open honestly. The nav render is fifty-two columns and
the bar at thirty-four fits a phone; the overview is ninety-two and scrolls
inside its own frame. It scrolls, but nothing says so.
