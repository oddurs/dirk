---
id: 64
title: Tests leave a session running when they fail
type: bug
status: done
milestone: v0.5
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: server
---

## Problem
Each session test ends with `quit(&session)`, which a failing assertion never
reaches.  A red run leaves one daemon per failed test on the machine, and they
are invisible unless you run `dirk session list` or `ps`.  Twenty of them
accumulated over an afternoon of debugging.

## Acceptance criteria
- [ ] A test that panics still ends its session
- [ ] Nothing is left running after a deliberately failing run
