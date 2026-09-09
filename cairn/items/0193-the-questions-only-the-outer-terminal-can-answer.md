---
id: 193
title: The questions only the outer terminal can answer
type: feature
status: backlog
milestone: v0.10
created: 2026-09-08
updated: 2026-09-08
priority: p3
effort: m
area: pty
---

## Problem

A pane now answers the questions it can answer for itself: who it is, where
its cursor is, which dirk this is. Two that fish 4 asks are not the pane's to
answer.

- `OSC 11 ; ?` asks the background colour. fish uses the reply to set
  `fish_terminal_color_theme` and pick a light or dark theme variant. Panes
  paint no ground of their own — the answer is the outer terminal's, and dirk
  does not know it.
- `DCS + q 696e646e ST` (XTGETTCAP for `indn`) asks whether the terminal can
  scroll content up. fish enables `scrollback-push` on ctrl-l when it can.
  vt100 implements `CSI Ps S` but offers no DCS callback, so the question is
  never seen.

Neither hangs anything: fish sends DA1 last and stops waiting when it comes
back. It only means fish inside dirk cannot follow the terminal's theme and
clears the screen instead of pushing it into scrollback.

## Proposal

For the colour: ask the outer terminal once at startup (crossterm has no
helper; it is an `OSC 11 ; ?` written to stdout and a reply read from stdin
alongside the key events), remember the answer, and hand it to any pane that
asks. Re-ask on `SIGWINCH` or a theme-change report, if the terminal sends
one. Over ssh the answer is still the terminal's, which is the right one.

For XTGETTCAP: take DCS out of the stream before vt100 sees it, the way
`graphics.rs` already takes the APC for images out, and answer `indn` and
nothing else.

## Acceptance criteria

- [ ] `fish_terminal_color_theme` inside dirk matches the terminal outside it
- [ ] `status test-terminal-features scroll-content-up` returns 0 in a pane
- [ ] A pane still gets no answer to a question dirk cannot stand behind
