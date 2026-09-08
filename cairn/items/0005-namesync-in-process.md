---
id: 5
title: namesync, in process
type: feature
status: done
milestone: v0.1
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: naming
---

## Problem
As a herdr plugin, namesync needed a socket client, a daemon, a state store and
sinks to do something dirk can do with a struct field.

## Proposal
Port `policy.js` and `naming.js` into `src/name.rs`. Titles arrive from the
pane's own OSC callback instead of an event subscription. No regex crate: every
pattern is a handful of character tests.

## Acceptance criteria
- [x] Hand-written names are never overwritten
- [x] Placeholders are adoptable
- [x] Rewordings are skipped on stemmed token overlap
- [x] Debounce and rate limit both hold
- [x] Junk titles never become names
