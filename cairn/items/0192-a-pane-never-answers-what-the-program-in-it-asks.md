---
id: 192
title: A pane never answers what the program in it asks
type: bug
status: doing
milestone: v0.10
assignee: Oddur Sigurdsson
claimed: 2026-09-08
created: 2026-09-08
updated: 2026-09-08
priority: p1
effort: m
area: pty
---

## What happens

Everything a program writes to its pane goes into vt100 and nothing ever comes
back. A terminal is meant to answer certain questions — Primary Device
Attributes (`CSI c`), a cursor position report (`CSI 6 n`), the terminal's name
and version (`CSI > q`) — and a program that asks one of them waits for the
reply.

fish 4 asks DA1 at every start and waits ten seconds before giving up with:

    warning: fish could not read response to Primary Device Attribute query
    after waiting for 10 seconds.

So every new fish shell in dirk takes ten seconds to reach its prompt. Neovim,
vim and anything else that probes its terminal on start is quieter about it but
does the same wait, or turns features off that it could have used.

## What should happen

A query from inside a pane is answered from inside dirk, on the pty, the way
any terminal answers it. The answer says what the pane's terminal is — a vt100
parser with colour, not xterm — and says nothing at all to a query for a
protocol dirk does not speak, which is how those protocols say "unsupported".

## Reproduction

1. `SHELL=fish dirk`
2. Wait. Ten seconds later the warning appears above the first prompt.

## Acceptance criteria

- [x] DA1, DA2, DA3, DSR, CPR and XTVERSION are answered
- [x] fish reaches its prompt without the warning
- [x] A query is answered only while the program is reading, so a program that
      floods the pane with queries cannot wedge dirk in `write`
- [x] A smoke test proves the answer comes back through the pty

## 2026-09-08

vt100 hands every CSI it does not implement to `Callbacks::unhandled_csi`, and
DA1, DA2, DSR, CPR and XTVERSION all arrive there, so the sink that already
caught titles now collects answers too. The reader thread takes them after each
chunk and sends them to the loop as `Ev::Answer`; the loop writes them.

Two decisions worth keeping:

- The loop writes, not the reader thread, and it polls the pty for `POLLOUT`
  first. A program that floods its pane with questions and never reads would
  fill the pty's input queue, and from there `write` blocks. The reader thread
  blocking is the deadlock `tests/smoke.rs` warns about, turned inside out. A
  terminal's driver drops input at that point; so does this.

- The kitty keyboard query and DECRQM get no answer. Silence is how those
  protocols say "unsupported", and it is safe only because DA1 *is* answered:
  every program that probes sends DA1 last and stops waiting when it returns.
  fish's exact startup sequence is `CSI ? u` then `CSI c`, and there is a unit
  test for that pair.

DA1 says VT220 with colour rather than xterm, because a program that believes
the xterm answer starts using things vt100 has never heard of. XTVERSION says
`dirk <version>` from Cargo, so a workaround fish keeps for some other terminal
is not applied here.

Still open, for another item: fish asks the background colour (`OSC 11 ; ?`)
to pick a light or dark theme and asks for `indn` through XTGETTCAP; neither is
answered. The first needs a question forwarded to the outer terminal and its
answer routed back, and the second needs a DCS callback vt100 does not offer.
