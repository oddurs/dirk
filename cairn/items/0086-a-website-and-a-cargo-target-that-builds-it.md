---
id: 86
title: A website, and a cargo target that builds it
type: feature
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: l
area: web
---

## Problem

dirk has a README and nothing else. Someone deciding whether to install it
reads 700 lines of Markdown on a grey page, or does not.

A website is the obvious answer and the obvious way to get it wrong: a second
copy of the documentation, written once, never updated, describing a version
of dirk that stopped existing three releases ago. The README is already the one
document that argues; a site that repeats it by hand will disagree with it
within a month.

## Proposal

`site/`, a workspace member. `cargo run -p site -- build` reads prose from
`site/content`, renders it through templates, and writes `site/dist`.

The reason it is a cargo target rather than an off-the-shelf generator is that
half of what belongs on the site is generated from the program: the palette
comes from `src/theme.rs`, the roadmap from cairn, the changelog from NEWS, and
the screenshots from the real binary on a real pseudo-terminal. Nothing that
reads Markdown from a directory can do any of that, so it would need a build
step in front of it anyway — and then there are two tools where there was one.

    make site         build into site/dist
    make site-serve   build, serve, and rebuild on change
    make shots        regenerate the terminal renders from the real binary

Dependencies are the usual answer: `pulldown-cmark` for Markdown, `minijinja`
for templates, `tiny_http` for the development server. What the crate owns is
the composition — the front matter, the navigation, the base-url rewriting and
the pipeline.

## Acceptance criteria

- [x] `make site` produces a complete static site from a clean checkout
- [x] `make site-serve` serves it and rebuilds when a file changes
- [x] Prose pages carry TOML front matter and are ordered into the nav by it
- [x] Every internal link and asset resolves under a base path, so the same
      build works at `/` locally and at `/dirk/` on GitHub Pages
- [ ] Nothing in the site is a hand-copied duplicate of something in the repo

## 2026-09-07

Done, except the last criterion, which is not, and is 0090.

The docs pages retype the key table and part of the configuration reference out
of the README. Everything else on the site is generated -- the palette, the
roadmap, the changelog, the screens -- so those two pages are the only place
the rule is broken, and they are the two most likely to go stale. 0090 carries
it; the shape is a page naming a source and a heading, the way `--skill` is
already generated from the command table.

The dev server was tested rather than assumed: it serves the pretty URLs Pages
serves, answers 404 with the 404 page, refuses a path with `..` in it, and
rebuilt and re-served a changed page while it was running.
