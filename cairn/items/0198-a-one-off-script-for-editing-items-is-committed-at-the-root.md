---
id: 198
title: A one-off script for editing items is committed at the root
type: chore
status: done
milestone: v0.10
assignee: Oddur Sigurdsson
created: 2026-09-08
updated: 2026-09-08
priority: p3
effort: s
area: packaging
---

## Problem

`w.py` sits in the root of the tree.  It is nineteen lines of Python for
rewriting an item's frontmatter in place, written to do one pass over the
backlog and committed by accident in #87 -- the roadmap commit it rode in on
had nothing to do with it.

Nothing references it, `cairn set` does what it does, and AGENTS.md tells every
agent reading it not to leave files like this behind:

> **Do not create ad-hoc TODO, PLAN or NOTES files.**  Create a cairn item
> instead.

A tree that contradicts its own brief in the first thing you see when you list
it teaches the next agent that the rule is optional.

## Proposal

Remove it.

## Acceptance criteria

- [x] `w.py` is gone from the tree
- [x] Nothing referenced it, checked rather than assumed
