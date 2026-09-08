+++
title = "Configuration"
description = "dirk runs with no configuration file. What you write overrides what it names and leaves the rest alone."
section = "Docs"
order = 30
+++

`~/.config/dirk/config.toml`, if it exists. `dirk session reload` re-reads it
without restarting, and an open board keeps its panes.

```toml
projects_root = "~/Code"     # where `o` looks
sidebar_width = 34
scrollback    = 5000
shell         = ""           # empty means $SHELL

default_agent = "claude"     # what `a` starts

[identity]                   # who the bar says you are
name    = ""                 # empty means $USER
session = "named"            # never | named | always
host    = "remote"           # never | remote | always

[nav]
glyphs    = "unicode"        # unicode | ascii
attention = "when-needed"    # when-needed | always | never
rows      = "tall"           # a second line carrying the branch
```

```callout trap
A board whose programs are not all on `PATH` is dropped at startup. An entry
that could only ever show `command not found` is worse than no entry.
```

`host = "remote"` asks the environment whether dirk is on the far side of an
`ssh` connection — the same question a shell prompt asks. It is worth saying: a
local session and one on a production machine are otherwise identical on
screen, which is how someone runs the right command in the wrong place.

The full reference, with every default, is in the README and the manual page —
`man dirk` once it is installed. This page is the shape of the file, not the
whole of it.
