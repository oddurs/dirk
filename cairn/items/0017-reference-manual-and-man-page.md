---
id: 17
title: Reference manual and man page
type: docs
status: backlog
milestone: v1.0
created: 2026-09-06
updated: 2026-09-06
priority: p2
area: docs
---

## Problem
The GNU layout is in place — COPYING, AUTHORS, NEWS, a Makefile — but the
documentation is a README. cairn ships doc/cairn.texi and a generated man page;
dirk ships neither, so `make install` installs a binary and nothing else.

## Acceptance criteria
- [ ] doc/dirk.texi, built to info and html by the Makefile
- [ ] A man page, and `make install-man` to put it where a system expects it
- [ ] The keymap and config reference live in the manual, with the README
      pointing at it rather than duplicating it
