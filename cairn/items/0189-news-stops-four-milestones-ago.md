---
id: 189
title: NEWS stops four milestones ago
type: docs
status: backlog
created: 2026-09-08
updated: 2026-09-08
priority: p1
area: docs
effort: m
---

## Problem

`NEWS` ends at 0.4.0. Since then v0.5, v0.6, v0.7 and v0.8 have all closed —
fifty-six items, most of them user-visible — and none of them has a line in the
changelog.

`HACKING` says release notes are cut from `NEWS` rather than written twice, and
that writing it is the first step of a release and not the last. So four
milestones of work cannot be released at all until this is written, and the
longer it waits the more of it is reconstructed rather than remembered.

## Proposal

Four sections, one per milestone, in the order the file already uses — the
versions in `NEWS` have tracked the milestone numbers since 0.1.0 and there is
no reason to break that now.

Written for somebody deciding whether to upgrade: what they can do that they
could not, and what changed under them. Not an item list. An entry that says
`0142` did something is an entry for the person who already knows.

## Acceptance criteria

- [x] One section, 0.5.0, covering all four — see below
- [x] `scripts/news` can cut each of them
- [x] Nothing in them describes a surface that does not exist

## 2026-09-08

One section rather than four, and the item was wrong to ask for four.

`NEWS` has tracked the milestone numbers exactly — 0.3.1 exists because v0.3.1
did — and the obvious move was to carry that on with 0.5.0 through 0.8.0. But
only one tag is going to be cut, and `release.yml` refuses to build unless the
tag, `Cargo.toml` and `NEWS` agree about the version. Four sections would have
been a changelog claiming three releases that never happened, which is a worse
kind of wrong than a large release.

So: everything since 0.4.0, organised by what it does rather than by which
milestone it arrived in. That is also the better read. Somebody deciding whether
to upgrade wants to know that they can search a pane now; which quarter of the
backlog that came from is dirk's business and not theirs.

Two things in the draft described surfaces that do not exist — `dirk notify`
(it is `dirk session notify`) and `session list --prune` (it is `session
prune`). Checking every claim against the source is most of the work of writing
one of these, and it found a third: `session prune` had no entry in the manual
page either, because it is answered by the client rather than by the session and
so the coverage guard added in `0179` cannot see it.

The date is the day this was written. `git work release 0.5.0` does the version
bump and `git work tag 0.5.0` cuts it, and both are somebody's decision rather
than this branch's.
