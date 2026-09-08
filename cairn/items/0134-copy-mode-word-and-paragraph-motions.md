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
- [ ] `w b e` and `W B E`, matching vi's word rules
- [ ] `{` `}` for paragraphs; blank lines are the boundary
- [ ] `ctrl-u` `ctrl-d` `ctrl-b` `ctrl-f`
- [ ] All of them extend an active selection
- [ ] A count prefix
