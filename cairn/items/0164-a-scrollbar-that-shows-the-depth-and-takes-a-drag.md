---
id: 164
title: A scrollbar that shows the depth and takes a drag
type: feature
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: chrome
depends_on:
- 156
- 157
---

## Problem
Scroll position is invisible and ungrabbable. A pane's scrollback has no
indicator at all -- being four lines back and four thousand look identical --
and the nav says `more below` in words on its last line, which is a sentence
where every other interface has a bar.

For a keyboard interface that is defensible: you scroll with keys and you know
where you are because you put yourself there. For a pointer it is not, because
the wheel is a gesture with no sense of distance. Ten flicks of a wheel could
be a screenful or a session's worth and nothing on screen distinguishes them.

## Proposal
A scrollbar where there is more than fits: the nav's sections, a pane being
read, and the overlays that list things -- the palette, search results, the
project picker.

It has to do three things, and the third is the one that makes it worth
drawing: say there is more, say how much and where in it you are, and take a
drag on the thumb. A bar that only shows position is an indicator; people will
try to grab it, and a bar that does not move under the pointer reads as broken.

One column. It goes in the space the interface already reserves at the right
edge, and it must not narrow anything: the nav elides names against an exact
budget and taking a column from that shortens every name in the list.

A pane that is live -- not being read back -- shows nothing. There is no
position to report, and a bar pinned to the bottom forever is furniture.

## Acceptance criteria
- [ ] A bar appears where there is more than fits, and not otherwise
- [ ] It shows both how deep the content is and where in it you are
- [ ] The thumb takes a drag, and the drag scrolls
- [ ] Clicking the trough pages toward the click
- [ ] It costs no width from the names beside it
- [ ] A live pane shows none
