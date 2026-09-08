---
id: 153
title: The changelog is one scroll with no way through it
type: feature
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p3
effort: s
area: web
---

## Problem

Twenty-five kilobytes of releases, oldest to newest, with no index, no dates in
view, and no way to reach a version except scrolling past every version after
it.

## Proposal

An index at the top: every release, its date, and its one-line summary, linking
to its own anchor. NEWS already opens each release with a sentence saying what
it is, which is exactly the summary that index wants.

## Acceptance criteria

- [x] Every release is reachable in one click from the top of the page
- [x] A release carries its date beside its version
- [x] The newest release is first

## 2026-09-07

An index of every release, newest first, with its date and its opening
sentence. Two of the five open straight into a heading with no lead paragraph,
so those take the heading — which is what the release is about, and better than
a blank cell.

`first_sentence` counts sentences rather than characters, and keeps taking them
until there is something to read: the first sentence of 0.4.0 is "Sessions.",
which is true and says nothing.
