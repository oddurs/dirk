---
id: 89
title: GitHub Pages, deployed by the workflow
type: chore
status: done
milestone: v1.0
assignee: oddurs
created: 2026-09-07
updated: 2026-09-07
priority: p1
effort: s
area: web
---

## Problem

A site that is built by hand and uploaded by hand is a site that is stale.

## Proposal

Pages, built from the workflow rather than from a branch. A push to main that
touches the site, the palette, the roadmap or NEWS rebuilds and deploys it; a
pull request builds it without deploying, so a change that breaks the build
fails in review rather than after the merge.

The base path is `/dirk/`, because these are project pages. It is a build
argument rather than a constant, so moving to a domain later is one flag.

## Acceptance criteria

- [x] A push to main deploys the site
- [x] A pull request builds it and does not deploy it
- [x] The deployment is a required check away from being wrong: the site build
      fails the pull request
- [x] Moving to a custom domain is a one-line change

## 2026-09-07

Pages is enabled and set to build from the workflow: https://oddurs.github.io/dirk/

Moving to a domain is `--base / --origin https://the.domain` and a CNAME file
in `site/assets`, which is copied through with everything else there. Both
were tested by building against a fake domain and checking the canonical links
and the sitemap came out absolute and correct.
