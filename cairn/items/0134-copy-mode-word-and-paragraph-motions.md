---
id: 134
title: 'Copy mode: word and paragraph motions'
type: feature
status: backlog
milestone: v0.8
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: mux
effort: s
---

## Problem
Copy mode moves by character, line, page and buffer end. Selecting an
identifier out of a stack trace means holding `l`. This is the gap people
actually feel, because every editor they use has `w`.

## Proposal
The tmux and vi set, no more: `w` `b` `e` for words, `W` `B` `E` for big words,
`{` and `}` for paragraphs, `ctrl-u` and `ctrl-d` for half pages, `ctrl-b` and
`ctrl-f` for whole ones.

Word boundaries follow vi's rule rather than a Unicode segmentation library —
what people expect here is what vi does, including where vi is arguably wrong.

## Acceptance criteria
- [x] `w b e` and `W B E`, matching vi's word rules
- [x] `{` `}` for paragraphs; blank lines are the boundary
- [x] `ctrl-u` `ctrl-d` `ctrl-b` `ctrl-f`
- [x] All of them extend an active selection
- [ ] A count prefix — not done; see below

## 2026-09-08

Three character classes rather than two, which is what makes `w` useful in code:
`foo.bar` is three words because vi stops between a word and the punctuation
beside it. Big words are anything that is not a space.

A motion runs along the line the cursor is on and stops at its ends. Crossing
lines would need the scrollback, and moving between rows is already the caller's
job — so the answer at the edge is to stand still, which is honest and is what
`0` and `$` already do.

No count prefix. It is a second mode inside a mode — every key has to become
"digit, or the thing it was" — and the motions themselves were the gap people
felt. Worth its own item if anybody misses it.

Selections extend for free: the anchor is untouched and every motion moves `at`,
which is what the selection is measured from.

The paging arms have to come before the word arm and the word arm has to require
no modifier, or `ctrl-b` is taken as a word motion — `b` matches whatever the
modifiers are unless you say otherwise, and a bare arm before a guarded one
silently wins.
