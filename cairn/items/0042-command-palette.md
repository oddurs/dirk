---
id: 42
title: Command palette
type: feature
status: done
milestone: v0.5
created: 2026-09-06
updated: 2026-09-07
priority: p1
area: nav
effort: m
---

## Problem
Every command is a key in a match arm and a line in the README. Discovering what
dirk can do means reading its source.

## Proposal
One list of every action, filtered by typing, over whatever you were doing —
the picker's mechanism pointed at commands instead of directories. It becomes
the answer to 'how do I' and removes the pressure to bind everything.

## Acceptance criteria
- [ ] Every action appears, with its binding shown beside it
- [ ] Subsequence filter, ranked, as the project picker already does
- [ ] Actions that cannot apply now are shown disabled with the reason
- [ ] Layouts, spaces and agents are reachable from it, so it doubles as a jump
