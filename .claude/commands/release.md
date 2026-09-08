---
description: Cut a release — NEWS first, then the version bump, then the tag
argument-hint: <major.minor.patch>
allowed-tools: Bash(git work:*), Bash(scripts/news:*), Bash(scripts/work:*), Bash(make:*), Bash(cargo:*), Bash(git:*), Bash(gh:*), Bash(cairn:*), Read, Edit, Grep, Glob
---

Cut release $ARGUMENTS.

NEWS is the changelog and the release notes are cut from it, so writing it is
the first step and not the last.

1. **Work out what is in it.** `git log --oneline $(git describe --tags
   --abbrev=0)..origin/main`, and the items closed since the last release.
2. **Write the NEWS section.** Use the `news` subagent. GNU form, wrapped at
   79, `*` for the release and `**` for headings under it. Say what changed
   from the user's side, name the traps, and keep the **Known limitations**
   section honest — every entry points at the backlog item that covers it.
3. **Check it.** `scripts/news $ARGUMENTS` must print your section and nothing
   else. That is exactly what the release page will show.
4. **Bump the manual page.** The `.TH` line in `doc/dirk.1` carries the
   version, and the release workflow refuses to build if it disagrees with
   `Cargo.toml`.
5. **`git work release $ARGUMENTS`** — branches, bumps `Cargo.toml` and the
   lock file, commits, and ships the pull request.
6. When it has landed: **`git work tag $ARGUMENTS`**. That pushes the tag,
   which is what builds and publishes the release. It will ask you to confirm,
   because it is public and it cannot be taken back.

Stop before step 6 and report, unless you were explicitly told to go all the
way. Publishing a release is not something to do on an assumption.
