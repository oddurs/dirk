---
id: 90
title: The documentation pages are a second copy of the README
type: feature
status: backlog
milestone: v1.0
depends_on:
- 86
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: m
area: web
---

## Problem

The site's scaffolding holds to its own rule everywhere except the place it
matters most. The palette is generated, the roadmap is generated, the changelog
is generated, the screens are generated — and then `site/content/docs/keys.md`
retypes the key table out of the README, and `configuration.md` retypes part of
the configuration reference.

Those are exactly the two things most likely to change and least likely to be
changed twice. Within a release one of the copies will be wrong, and it will be
the one nobody was looking at.

## Proposal

The README is already the document that argues, and it is already sectioned.
Take the docs pages from it rather than beside it.

The shape that probably works: a page whose front matter names a source and a
heading — `source = "README.md"`, `heading = "Keys"` — and the generator lifts
that section, rewrites its links, and renders it under the page's own title.
The page keeps its own prose above and below; only the part that exists twice
is taken.

The harder half is the configuration reference, which should not really live in
the README either. `src/config.rs` knows every key, its default, and in several
cases why the default is what it is. `dirk --skill` is already generated from
the command table for exactly this reason and is the precedent to follow.

## Acceptance criteria

- [ ] No key, default or command appears in `site/content` that also exists in
      the repository
- [ ] Changing a keybinding in the source changes it on the site with no edit
      to `site/`
- [ ] The README stays the document a person reads first; the site does not
      become the source it is taken from
