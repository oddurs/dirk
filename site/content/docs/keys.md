+++
title = "Keys"
description = "The prefix is Ctrl-Space, and everything else goes straight through to the program in the pane."
section = "Docs"
order = 20
+++

The prefix is <kbd>Ctrl</kbd>+<kbd>Space</kbd>. <kbd>Ctrl</kbd>+<kbd>a</kbd>
and <kbd>Ctrl</kbd>+<kbd>b</kbd> are both load-bearing in every shell line
editor; Ctrl-Space is NUL, which nothing sends on purpose. Press it twice to
send a literal one through.

## After the prefix

| Key | |
| --- | --- |
| `n` | New workspace in this project |
| `o` | Open a project |
| `x` | Close the focused pane |
| `\|` `v` | Split into columns |
| `-` `s` | Split into rows |
| `;` | Next pane in this workspace |
| `Tab` `j` `k` | Next / previous workspace |
| `1` `2` `3` | Jump to a board |
| `b` | Hide the nav |
| `u` | Release a held name |
| `r` | Restart a stopped pane |
| `w` | Give the nav the keyboard |
| `a` | Start an agent here |
| `[` | Read this pane's scrollback, and copy out of it |
| `/` | Find a line, in any pane |
| `d` | Detach |
| `q` | Quit |

## Leaving and quitting are different things

A session outlives the terminal it was started in, so detaching is the ordinary
way out: press `d`, and come back to the same panes with `dirk`. Quitting ends
the work.

The bar carries both in words for that reason — a single `✕` cannot say which
one it is — and `✕ quit` takes two clicks, because it sits at the edge of the
screen where a stray click is most likely.

<kbd>Esc</kbd> never leaves. It closes the picker, or hands the keyboard back
from the nav, and does nothing else.
