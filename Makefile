# dirk — convenience targets. The build itself is cargo's job; this exists for
# what cargo does not cover: the checks CI runs, the manual page, installing
# into a prefix a system expects, and making a tarball someone can build from.
PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
MANDIR  ?= $(DATADIR)/man
DOCDIR  ?= $(DATADIR)/doc/dirk
CARGO   ?= cargo
INSTALL ?= install

DIRK    := target/release/dirk
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DISTDIR := dirk-$(VERSION)

.PHONY: all build check test fmt lint shot roadmap setup news ChangeLog dist \
        install install-man uninstall clean distclean help

all: build

build:
	$(CARGO) build --release

# ─── the gate ────────────────────────────────────────────────────────────────

# Everything CI runs. If this passes, the pull request should be green.
check: fmt lint test

fmt:
	$(CARGO) fmt --check

lint:
	$(CARGO) clippy --all-targets -- -D warnings

# The unit tests cover the naming policy; tests/smoke.rs drives the real binary
# on a real pseudo-terminal, so this is one entry point for both.
test:
	$(CARGO) test

# ─── the loop ────────────────────────────────────────────────────────────────

# One command for a fresh checkout: the `git work` alias, the hooks that keep
# tool attribution out of the history, and the git settings the workflow
# assumes. HACKING has the rest.
setup:
	@scripts/work setup

# Print what dirk currently paints, as plain text. The fastest way to see
# whether a change to the chrome did what you meant.
#
# The build is not optional: the example spawns the built binary rather than
# linking it, and `cargo run --example` will not rebuild that for you. Without
# this line the shot happily shows you the last version of the chrome.
shot:
	$(CARGO) build
	$(CARGO) run --example shot

# Regenerate the roadmap from the backlog.
roadmap:
	cairn render

# The release notes for the current version, as the release workflow cuts them.
news:
	@scripts/news $(VERSION)

# ─── the changelog ───────────────────────────────────────────────────────────

# GNU expects a ChangeLog; git already is one, and two of them drift. This
# generates the file from the history at distribution time rather than keeping
# a second copy under version control. NEWS is the one people read.
ChangeLog:
	@git log --date=short \
	    --format='%ad  %aN  <%aE>%n%n%w(76,8,8)* %s%n%n%w(76,8,8)%b' \
	  | sed -e '/^[[:space:]]*[Cc]o-[Aa]uthored-[Bb]y:/d' \
	        -e '/^[[:space:]]*.*[Gg]enerated with \[*[Cc]laude/d' \
	        -e 's/[[:space:]]*$$//' \
	  | cat -s > $@
	@echo "wrote $@ from the history"

# ─── distribution ────────────────────────────────────────────────────────────

# A tarball a stranger could build from: the tree as git has it, plus the
# generated ChangeLog, which is not under version control.
dist: ChangeLog
	@rm -rf $(DISTDIR) $(DISTDIR).tar.gz
	@git archive --format=tar --prefix=$(DISTDIR)/ HEAD | tar -xf -
	@cp ChangeLog $(DISTDIR)/ChangeLog
	@tar -czf $(DISTDIR).tar.gz $(DISTDIR)
	@rm -rf $(DISTDIR)
	@echo "wrote $(DISTDIR).tar.gz"

# ─── installation ────────────────────────────────────────────────────────────

install: build install-man
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL) -m 755 $(DIRK) $(DESTDIR)$(BINDIR)/dirk
	$(INSTALL) -d $(DESTDIR)$(DOCDIR)
	$(INSTALL) -m 644 README.md NEWS AUTHORS THANKS COPYING $(DESTDIR)$(DOCDIR)

install-man:
	$(INSTALL) -d $(DESTDIR)$(MANDIR)/man1
	$(INSTALL) -m 644 doc/dirk.1 $(DESTDIR)$(MANDIR)/man1/dirk.1

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/dirk
	rm -f $(DESTDIR)$(MANDIR)/man1/dirk.1
	rm -rf $(DESTDIR)$(DOCDIR)

# ─── tidying ─────────────────────────────────────────────────────────────────

clean:
	$(CARGO) clean

distclean: clean
	rm -rf ChangeLog $(DISTDIR) dirk-*.tar.gz

help:
	@echo 'build      cargo build --release'
	@echo 'check      fmt, clippy and the full suite — the gate CI enforces'
	@echo 'setup      configure git for the worktree workflow (once per checkout)'
	@echo 'shot       print what dirk currently paints, as plain text'
	@echo 'roadmap    regenerate ROADMAP.md from the backlog'
	@echo 'news       the release notes for $(VERSION), cut from NEWS'
	@echo 'dist       dirk-$(VERSION).tar.gz, buildable from source'
	@echo 'install    into $(PREFIX) — honours PREFIX and DESTDIR'
	@echo 'uninstall  take it back out again'
