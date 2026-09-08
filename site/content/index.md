+++
title = "dirk"
description = "A terminal multiplexer that knows what its sessions are for: a clickable project tree, static boards, and workspaces named from what the program inside them is doing."
template = "index.html"
toc = false
+++

<section class="hero">
  <h1>A terminal multiplexer that knows what its sessions are for.</h1>
  <p class="lede">tmux gives you panes and asks you to remember what is in them.
  dirk starts from the opposite end: the sidebar is the product, and the panes
  hang off it.</p>
  <div class="hero-actions">
    <a class="button" data-variant="primary" href="/docs/install/">Install</a>
    <a class="button" href="https://github.com/oddurs/dirk">Source</a>
    <span class="command"><span class="prompt">$</span><code>cargo install --git https://github.com/oddurs/dirk</code></span>
  </div>
</section>

```shot overview
```

<p class="lede">Every terminal on this page is real output from the real
binary, on a real pseudo-terminal — not a screenshot. Select it.</p>

## Three ideas, and keeping them apart is the whole design

<div class="cards">
  <article class="card">
    <h3>The nav is ordered by what needs you</h3>
    <p>Boards are what you glance at, spaces are where the work lives, and
    between them sits what is interrupting. It occupies no rows at all when
    nothing does.</p>
  </article>
  <article class="card">
    <h3>The nav lists what is open</h3>
    <p>Not what exists. A list of ninety directories is a file browser. A
    project appears once it has a workspace and disappears when its last one
    closes.</p>
  </article>
  <article class="card">
    <h3>Boards are not workspaces — except that they are</h3>
    <p>A named arrangement of programs, built the first time you open one
    rather than running in the background so it can occasionally be glanced
    at.</p>
  </article>
</div>

## The nav is the product

Three zones, ordered by what is likely to need you. Boards at the top are what
you glance at; spaces in the middle are where the work lives; between them sits
whatever is interrupting — and that zone occupies no rows at all when nothing
is.

A workspace is named from what the program inside it says it is doing. Not from
the directory it was opened in, which you already knew.

```shot nav
```

## It gives things up in an order

The bar is the one surface that spans the whole terminal, and most of what it
does is decide what to lose as the terminal narrows. That is a list, applied in
order, each step giving up strictly less than the one before — so it shrinks
without ever trading something you need for something you do not.

```shot rail-wide
```
```shot rail-mid
```
```shot rail-narrow
```

The clock goes first. Then the pane counter, the project, the exit words. What
is owed is not on the list at all: `blocked` is the only state waiting on a
human, so it is the last thing on the screen rather than the first thing cut.

## It knows what the agents are doing

dirk reads what is running in each pane from its foreground process group, so a
shell is a shell and an agent is an agent. The lifecycle states are real:
<span class="state" data-state="working">working</span>,
<span class="state" data-state="blocked">blocked</span>,
<span class="state" data-state="done">done</span> and
<span class="state" data-state="idle">idle</span>.

`blocked` is the only one waiting on a human, and it is the only one drawn in
red. Everything about the attention zone follows from that distinction.

```callout note
A workspace is named from what the program inside it says it is doing, not from
the directory it was opened in. `Building the mux core` is a name; `~/Code/dirk`
is a path you already knew.
```

## Sessions outlive their terminal

`dirk` attaches, starting the session if it is not running. Close the terminal
and everything in it keeps going. There is a socket API and a CLI that speaks
it, so an agent inside a pane can drive the session it is running in.

[Read the documentation](/docs/install/) · [See the roadmap](/roadmap/)
