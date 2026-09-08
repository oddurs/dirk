---
id: 159
title: Who owns the mouse inside a pane
type: spike
status: backlog
milestone: v0.11
created: 2026-09-08
updated: 2026-09-08
priority: p0
effort: s
area: mux
---

## Question
When the pointer is inside a pane running a program that asked for mouse
reporting, who gets the event -- dirk, or the program?

## Why it needs answering before the work
Today it is all-or-nothing per pane: `pane_wants_mouse` is true and dirk hands
over the wheel, selection and everything else. Inside `nvim` or `htop` you
cannot select text with the pointer, cannot scroll dirk's scrollback, and
cannot start a drag that happens to begin over that pane.

That is a defensible answer -- the alternative of stealing gestures from the
program is worse -- and it is not obviously the right one for a product whose
premise is that pointing works everywhere. Every gesture in this milestone that
begins inside a pane has to know the answer:

- Dragging a split border, when the border is drawn against a pane that wants
  the mouse.
- Selecting text over a full-screen program, which is the single most common
  thing people ask a multiplexer for.
- Right-click, which is currently forwarded and would otherwise be the context
  menu.
- Hover, which is motion, which such a pane is asking for.

Deciding it per item means deciding it four times and differently.

## What would settle it
What the neighbours do, and why:

- **tmux** forwards to the program and reserves nothing; holding Shift makes
  the *outer terminal* handle it instead, which works because tmux never sees
  those events at all.
- **iTerm2** and **kitty** use a modifier to mean "this one is for the chrome".
- **screen** does not have the problem, because it does not have the mouse.

The candidate answer is a modifier -- most likely Alt, since Shift is spoken
for by the outer terminal and Ctrl is heavily used inside programs -- that
means "this gesture is dirk's", plus a small set of gestures dirk keeps
unconditionally because no program can reasonably want them: a press on a
border, on the divider, on a scrollbar.

What settles it is trying it against the four cases above and against `nvim`,
`htop` and `less`, and writing down which gestures dirk keeps without a
modifier and why each one is safe to keep.

## Answer

<!-- Filled in when the spike closes. A spike with no answer recorded was a
     waste of the time it took. -->
