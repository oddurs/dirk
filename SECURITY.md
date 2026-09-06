# Security policy

## Reporting a vulnerability

Please report privately through GitHub's
[private vulnerability reporting](https://github.com/oddurs/dirk/security/advisories/new)
rather than opening a public issue.

You should get an acknowledgement within a week. If a fix is warranted, expect
a release within thirty days of confirmation, and credit in `NEWS` unless you
would rather not be named.

## Scope

dirk spawns processes on pseudo-terminals, parses whatever they write, and
reads a configuration file. Things worth reporting:

- Terminal output from a program in a pane causing memory unsafety, a panic
  that leaves the terminal unusable, or execution of anything the user did not
  start. A pane's output is untrusted input: it can come from a remote host
  over `ssh`, or from a program processing a hostile file.
- An escape sequence from inside a pane reaching the outer terminal unfiltered
  in a way that changes its state or writes to the user's clipboard.
- A crafted `config.toml` causing dirk to run a program the user did not
  configure.
- A pane inheriting credentials or a file descriptor it should not have.

## Not in scope

- dirk runs the programs you tell it to, including the shell in `$SHELL` and
  the pages in `config.toml`. That is the whole point of it, and a page
  configured to run something harmful is not a vulnerability in dirk.
- Anything a program running in a pane does to files the user can already
  write.
- Denial of service by opening panes until the machine runs out of resources.
