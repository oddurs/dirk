---
id: 151
title: The roadmap page is a wall of text
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

The page that proves this project is real is the least readable thing on the
site. It is 184 lines of `0001 Adopt cairn for the roadmap feature`, one after
another, with cairn's ASCII progress bars rendered as literal `##########` and
nothing to tell a milestone from an item from a status heading.

The cause is a lossy round trip. cairn holds structured items and renders them
to Markdown; the site then parses that Markdown back into HTML and has nothing
left to work with — no type, no priority, no area, no dates, no dependencies.
It is reading a picture of the data.

## Proposal

Read the backlog itself. `cairn/items/*.md` is a directory of files with simple
`key: value` front matter and a Markdown body, which needs about fifty lines to
parse and no toolchain in CI — the same trade `src/theme.rs` already gets, for
the same reason.

Then the page can be what it is: milestones in dependency order, each with real
progress; items as rows carrying their type, priority and area; open work
separated from done; and every item linking to its own file, because the point
of keeping a backlog in the repository is that you can read the reasoning.

The generated `ROADMAP.md` stays what it is — the file you read in a checkout.
This is the other view of the same items, and both come from one source.

## Acceptance criteria

- [x] The page is generated from `cairn/items`, not from `ROADMAP.md`
- [x] A milestone shows its progress as something other than `##########`
- [x] An item shows its type, priority and area, and links to its own file
- [x] What is being worked on now is visible without reading the whole page
- [x] A change to cairn's on-disk format fails the build with a message rather
      than rendering an empty page

## 2026-09-07

The page reads `cairn/items` directly. Fifty lines of front-matter parsing, no
new dependency, and nothing to install in CI — which is the reason it is not
`cairn export`: the tool that owns the format would cost minutes on every push
to answer a question the files already answer.

Milestones sort by their own `depends_on`, because a milestone is the thing
that has to be true before the next one is worth starting and that order is
already recorded. A finished one opens collapsed: what is left is what the page
is for, and eighty rows of shipped work in front of it is a history lesson
nobody asked for — one click away, because it is also the evidence.

Items carry the marks the nav draws beside an agent. A status is a status
wherever it is read.

`cairn.toml`s format is checked against what this parses, so a format change
fails the build with a message rather than rendering a project with no plans.
