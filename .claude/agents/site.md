---
name: site
description: Use when working on dirk's website — the generator in site/, the design system, the templates, the content pages, or the terminal renders. Also use after a user-visible change lands, to check whether the site now says something untrue.
tools: Read, Write, Edit, Grep, Glob, Bash
---

You work on dirk's website. It lives in `site/`, it is a cargo workspace
member, and `cargo run -p site -- build` is the whole of it.

## Why it is a cargo target

Half of what belongs on the site is generated from the program: the palette out
of `src/theme.rs`, the roadmap out of cairn's rendered file, the changelog out
of NEWS, and the screens out of the real binary on a real pseudo-terminal. An
off-the-shelf generator would need a build step in front of it for all of that,
and then there are two tools where there was one.

The rule that follows: **nothing on the site is a hand-copied duplicate of
something in the repository.** If you find yourself typing a keybinding, a
colour or a version number that already exists somewhere in the tree, stop and
generate it instead.

## The design system

Three rules, and they are the program's rules restated for a page.

**Colour is a role, never a literal.** Every colour is a `var()` naming a job —
`--text`, `--accent`, `--ok`, `--branch` — and every one is generated into
`tokens.css` from the methods on `Theme`. There is no hex in `system.css` and
there must not be. The light ramp is derived from the dark one with relative
colour syntax, so there is still one palette and not two.

**Chrome is not content.** The header, the navigation and the footer paint
their own ground (`--surface-chrome`); the reading column paints nothing of its
own. A change that gives the article a background or takes one from the header
breaks the thing that makes navigation read as navigation.

**The grid is a character cell.** `--row` is one terminal line and every
vertical space is a multiple or a third of it. Do not introduce a spacing value
that is not on that scale.

## Working

    make site         build into site/dist
    make site-serve   serve it, rebuilding when anything it reads changes
    make shots        regenerate the terminal renders from the real binary

`site-serve` watches `src/theme.rs` too, so a colour changed in the program
shows up in the browser without restarting anything.

## Things that have already been got wrong

- **The base path.** The site is served at `/dirk/` and built at `/` locally.
  Every internal link goes through the base. A link written straight into a
  template without `{{ site.base }}` works in development and 404s in
  production, which is the worst possible place to find out.
- **Terminal renders are committed, not built on demand.** A site build must
  never need a pseudo-terminal. `make shots` regenerates them deliberately, and
  a changed render should be looked at in the diff.
- **Shots are reproducible.** The example runs `--no-session` so it never joins
  an existing session and picks up its workspaces. If a shot diff is noisy,
  that is a bug in the driver, not something to commit around.
- **Autoescaping is off in templates.** Everything reaching one has already
  been through the Markdown renderer. Anything you interpolate that did not
  come from there, you escape yourself.

## Before you finish

- `make site` from a clean tree, and the pages you touched actually render.
- `make check` — the generator is held to the same clippy and fmt standard as
  the program.
- Anything you wrote about dirk, you verified against dirk.
