---
id: 84
title: The GNU furniture the project still lacks
type: docs
status: doing
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p2
effort: m
area: docs
---

## Problem

The GNU layout is half laid: COPYING, AUTHORS, NEWS, SECURITY and a Makefile
are here; INSTALL, THANKS, HACKING and a ChangeLog are not. `make install`
installs a binary and no manual page, so a packager has nothing to install into
`man1` and a user has nothing to read but the README.

Around the edges, the things a public repository is judged on are missing too:
no code of conduct, no citation file, no editorconfig, no gitattributes, no
mailmap, no owners file.

## Proposal

- `INSTALL` — requirements, a release tarball, a checkout, `make install` with
  PREFIX and DESTDIR, and what a packager needs to know. Not the autotools
  boilerplate, which would be a lie about a cargo build.
- `HACKING` — the development loop from a fresh clone: `make setup`, the
  worktree verbs, the gate, how to test a program that draws a terminal, and
  how a release is cut.
- `THANKS` — the libraries the hard parts were delegated to, and the projects
  the ideas came from.
- `ChangeLog` — generated from git in GNU form by `make ChangeLog`, and
  included in the distribution tarball. Never hand-edited; NEWS is the file
  people read.
- `doc/dirk.1` — a manual page, installed by `make install`, removed by
  `make uninstall`.
- `CODE_OF_CONDUCT.md`, `CITATION.cff`, `.editorconfig`, `.gitattributes`,
  `.mailmap`, `.github/CODEOWNERS`.

The reference manual in texinfo stays where it is, in 0017; this is the
furniture around it.

## Acceptance criteria

- [ ] INSTALL, HACKING and THANKS exist and are true of this build
- [ ] `make ChangeLog` produces GNU-form entries from the history
- [ ] `make install` installs the binary and the manual page; `make uninstall`
      removes both
- [ ] `make dist` produces a tarball a stranger could build from
