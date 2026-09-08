---
id: 90
title: The documentation pages are a second copy of the README
type: feature
status: doing
milestone: v1.0
assignee: oddurs
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
- [x] Changing a keybinding in the source changes it on the site with no edit
      to `site/`
- [x] The README stays the document a person reads first; the site does not
      become the source it is taken from

## 2026-09-07

A fence, not front matter: ```from README.md Keys``` lifts one `## ` section
out of a document already in the repository and drops it where the page puts
it. A fence because it can appear more than once and be placed exactly, and
because it is still legible as plain Markdown, which is how a page is read in a
pull request.

The keys page and the configuration page are now taken rather than typed, and
lifting them proved the point immediately: the copy did not have the command
palette in it, which landed on main while the copy sat there.

A heading that has been renamed fails the build by name — `README.md has no
section called "Keystrokes"` — rather than rendering a page with a hole in it.

The first criterion stays open, and honestly. The install page is still
authored: what it duplicates is `INSTALL`, whose headings are underlined plain
text rather than `## `, so lifting it needs a second parser for a second format
and would produce text where the page wants Markdown. The duplication that
mattered is the reference — keys and configuration change with the code, and an
install guide changes when the way to install changes, which is rarer and
louder.

The second half of the item — taking the configuration reference from
`src/config.rs` the way `--skill` is taken from the command table — is not done
either. The README is the source now, which is one source instead of two; the
program would be zero.
