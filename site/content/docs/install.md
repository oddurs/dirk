+++
title = "Install"
description = "A binary, a checkout, or a package. Whichever, it is one file with no runtime dependencies."
section = "Docs"
order = 10
+++

## A release binary

Releases carry pre-built tarballs for linux and macOS, on both architectures.
Each holds the binary, the manual page, and the files you need to know what you
have and what you may do with it.

```console
$ tar -xzf dirk-0.4.0-aarch64-apple-darwin.tar.gz
$ sudo install -m 755 dirk-*/dirk /usr/local/bin/dirk
$ sudo install -m 644 dirk-*/dirk.1 /usr/local/share/man/man1/dirk.1
```

Every release also carries `SHA256SUMS`. Verify before you install:

```console
$ sha256sum --check --ignore-missing SHA256SUMS
```

## From a checkout

Rust 1.88 or newer. dirk uses let-chains throughout, which is what sets the
floor.

```console
$ git clone https://github.com/oddurs/dirk && cd dirk
$ make && sudo make install
```

`make install` honours `PREFIX` and `DESTDIR`, and `make uninstall` removes
exactly what it put there.

## Without installing anything

```console
$ cargo install --git https://github.com/oddurs/dirk
```

```callout note
There is no crates.io release. The name `dirk` was taken in 2019 by an
unrelated tool, and publishing under a name that does not match the binary is
worse than not publishing.
```

## What you need

- **Rust 1.88** or newer, with cargo, if you are building.
- **A terminal** that understands 256 colours, and mouse reporting if you want
  the sidebar to be clickable. Any terminal written this century does.
- **Linux or macOS.** Both are tested on every change; nothing else is, which
  is a statement about the tests rather than about portability.
