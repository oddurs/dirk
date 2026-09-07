---
id: 57
title: The rest of the naming configuration
type: feature
status: done
milestone: v0.3.1
assignee: oddurs
created: 2026-09-06
updated: 2026-09-06
priority: p1
area: naming
effort: m
---

## Problem
Five of namesync's knobs are hardcoded in the port: the similarity threshold,
whether hand-written names are respected, whether to hold off while blocked,
whether to strip the project prefix, and the list of titles never to use.

`ignore_titles` is the one that matters most in practice. It is how you say
that `claude`, `nvim` and `lazygit` are programs rather than intents, and the
port has no equivalent — those get through as names.

## Acceptance criteria
- [x] similarity_threshold, respect_manual_names, skip_while_blocked,
      strip_project_prefix, ignore_titles all configurable
- [x] ignore_titles ships with namesync's list
- [x] targets: workspace and agent, each switchable
- [x] Every default matches what the port does today, so configuring nothing
      changes nothing
