---
id: 145
title: dirk update
type: feature
status: backlog
milestone: v0.9
created: 2026-09-07
updated: 2026-09-07
priority: p2
area: packaging
effort: m
---

## Problem
Upgrading is: find the release page, pick the right triple, download, untar,
`sudo install`. Six steps and a chance to get the architecture wrong, which is
why people stay several versions behind.

## Proposal
`dirk update` checks the latest release, compares it to the running version,
downloads the asset for this platform, verifies it against the published
checksum, and replaces the binary in place. `--check` says what it would do.

Refuse, clearly, where dirk did not install itself. Homebrew and nix own their
copies and a binary swapped underneath them is a broken installation two weeks
later; name the package manager and print its command instead.

One channel. Two is a project with a release manager, and this one does not have
one.

## Acceptance criteria
- [ ] `--check` reports the available version and does nothing else
- [ ] The asset for the running platform, verified against its checksum
- [ ] Atomic replacement; a failure leaves the working binary in place
- [ ] Package-manager installs are detected and refused with their own command
- [ ] Says whether a running session needs restarting, and how
