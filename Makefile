# dirk — convenience targets. The build itself is cargo's job; this exists for
# what cargo does not cover: the checks CI runs, the manual page, installing
# into a prefix a system expects, and making a tarball someone can build from.
PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
# Where each shell looks. Separate variables because distributions disagree
# about all three, and a packager should not have to patch a recipe to move one.
BASHDIR ?= $(DATADIR)/bash-completion/completions
ZSHDIR  ?= $(DATADIR)/zsh/site-functions
FISHDIR ?= $(DATADIR)/fish/vendor_completions.d
DATADIR ?= $(PREFIX)/share
MANDIR  ?= $(DATADIR)/man
DOCDIR  ?= $(DATADIR)/doc/dirk
CARGO   ?= cargo
INSTALL ?= install

DIRK    := target/release/dirk
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DISTDIR := dirk-$(VERSION)

.PHONY: all build check test fmt lint shellcheck milestones shot shots site site-serve roadmap api setup \
        news ChangeLog dist install install-man install-completions uninstall clean distclean help

all: build

build:
	$(CARGO) build --release

# ─── the gate ────────────────────────────────────────────────────────────────

# Everything CI runs. If this passes, the pull request should be green.
check: fmt lint test

# --all and --workspace: the site generator is a member and is held to the
# same standard as the program. It ships too.
fmt:
	$(CARGO) fmt --all --check

lint: shellcheck milestones conflicts
	$(CARGO) clippy --workspace --all-targets -- -D warnings

# A conflict marker is never intentional and is trivial to see, and `cargo fmt`
# only sees the ones in Rust. A manual page with three of them in the middle of
# its COMMANDS section passed `mandoc -T lint` and was committed, because the
# eye reads the prose around them and skips the noise.
#
# Tracked files only, so a marker in something untracked is somebody's business
# rather than the gate's.
conflicts:
	@if git grep -n -I -E "^(<{7}|={7}|>{7}|\|{7})( |$$)" -- . ':!Makefile'; then \
		echo "conflicts: a merge marker is committed above" >&2; \
		exit 1; \
	fi

# The scripts are the workflow, and CI holds them to this. It was not in the
# gate, so the first thing to fail on it was a change to the script that runs
# the gate. Skipped rather than fatal when shellcheck is absent -- CI is the
# authority and says so out loud rather than passing in silence.
SCRIPTS = scripts/work scripts/news scripts/milestones scripts/dev .githooks/commit-msg .githooks/pre-push

# A milestone is as done as the work in it, and cairn's hook keeps that true as
# changes happen. This catches the case the hook cannot: somebody editing an
# item file by hand, or a merge bringing two branches' items together.
milestones:
	@scripts/milestones --check

shellcheck:
	@if command -v shellcheck >/dev/null 2>&1; then \
		echo "shellcheck -S warning $(SCRIPTS)"; \
		shellcheck -S warning $(SCRIPTS); \
	else \
		echo "shellcheck: not installed, skipping -- CI runs it and will not"; \
	fi

# The unit tests cover the naming policy; tests/smoke.rs drives the real binary
# on a real pseudo-terminal, so this is one entry point for both.
test:
	$(CARGO) test --workspace

# ─── the loop ────────────────────────────────────────────────────────────────

# One command for a fresh checkout: the `git work` alias, the hooks that keep
# tool attribution out of the history, and the git settings the workflow
# assumes. HACKING has the rest.
setup:
	@scripts/work setup

# A session that follows the build: rebuilds on every change to the source and
# hands the running `dev` session over to the result, keeping every shell and
# agent in it. Attach from anywhere with `dirk-dev`.
dev:
	@scripts/dev

# Print what dirk currently paints, as plain text. The fastest way to see
# whether a change to the chrome did what you meant.
#
# The build is not optional: the example spawns the built binary rather than
# linking it, and `cargo run --example` will not rebuild that for you. Without
# this line the shot happily shows you the last version of the chrome.
shot:
	$(CARGO) build
	$(CARGO) run --example shot

# ─── the website ─────────────────────────────────────────────────────────────

# The base path the built site will be served under. GitHub Pages serves this
# project at /dirk/; a local build is at the root. It is a variable rather than
# a constant so moving to a domain is one flag and not a search.
SITE_BASE ?= /

site:
	$(CARGO) run --quiet -p site -- build --base $(SITE_BASE)

# Serves site/dist and rebuilds when anything it reads changes, including
# src/theme.rs -- a colour changed in the program shows up in the browser.
site-serve:
	$(CARGO) run --quiet -p site -- serve

# The terminal renders the site shows: real output from the real binary. They
# are committed rather than built on demand, so a site build never needs a
# pseudo-terminal and a change to the chrome shows up in a diff.
#
# The build is not optional, for the same reason `shot` needs it: the example
# spawns the built binary rather than linking it.
shots:
	$(CARGO) build
	$(CARGO) run --quiet --example shot -- --html > site/shots/overview.html
	@# The bar at three widths: what it gives up as the terminal narrows is
	@# most of what it does, and one row of it says that without twenty-five
	@# rows of unchanged scrollback around each one.
	DIRK_SHOT_SIZE=26x92 DIRK_SHOT_ROWS=25-25 $(CARGO) run --quiet --example shot -- --html > site/shots/rail-wide.html
	DIRK_SHOT_SIZE=26x56 DIRK_SHOT_ROWS=25-25 $(CARGO) run --quiet --example shot -- --html > site/shots/rail-mid.html
	DIRK_SHOT_SIZE=26x34 DIRK_SHOT_ROWS=25-25 $(CARGO) run --quiet --example shot -- --html > site/shots/rail-narrow.html
	@# The nav, at a width where it is most of the screen rather than a third.
	DIRK_SHOT_SIZE=26x52 $(CARGO) run --quiet --example shot -- --html > site/shots/nav.html
	@ls -1 site/shots/*.html



# Regenerate the roadmap from the backlog.
roadmap:
	cairn render

# The socket surface, as data. Checked in so that changing it is a diff rather
# than a surprise for whoever was speaking the old one; a test fails when this
# has not been run.
api:
	$(CARGO) run --quiet -- api schema > doc/api.json

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

install: build install-man install-completions
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL) -m 755 $(DIRK) $(DESTDIR)$(BINDIR)/dirk
	$(INSTALL) -d $(DESTDIR)$(DOCDIR)
	$(INSTALL) -m 644 README.md NEWS AUTHORS THANKS COPYING $(DESTDIR)$(DOCDIR)

# Generated by the binary that was just built, so what is installed describes
# the surface that shipped with it rather than the one at the last commit
# somebody remembered to regenerate a file on.
install-completions: build
	$(INSTALL) -d $(DESTDIR)$(BASHDIR) $(DESTDIR)$(ZSHDIR) $(DESTDIR)$(FISHDIR)
	$(DIRK) completion bash > $(DESTDIR)$(BASHDIR)/dirk
	$(DIRK) completion zsh  > $(DESTDIR)$(ZSHDIR)/_dirk
	$(DIRK) completion fish > $(DESTDIR)$(FISHDIR)/dirk.fish

install-man:
	$(INSTALL) -d $(DESTDIR)$(MANDIR)/man1
	$(INSTALL) -m 644 doc/dirk.1 $(DESTDIR)$(MANDIR)/man1/dirk.1

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/dirk
	rm -f $(DESTDIR)$(MANDIR)/man1/dirk.1
	rm -f $(DESTDIR)$(BASHDIR)/dirk $(DESTDIR)$(ZSHDIR)/_dirk $(DESTDIR)$(FISHDIR)/dirk.fish
	rm -rf $(DESTDIR)$(DOCDIR)

# ─── tidying ─────────────────────────────────────────────────────────────────

clean:
	$(CARGO) clean

distclean: clean
	rm -rf ChangeLog $(DISTDIR) dirk-*.tar.gz site/dist

help:
	@echo 'build      cargo build --release'
	@echo 'check      fmt, clippy and the full suite — the gate CI enforces'
	@echo 'setup      configure git for the worktree workflow (once per checkout)'
	@echo 'dev        a running session that follows the build; attach with dirk-dev'
	@echo 'shot       print what dirk currently paints, as plain text'
	@echo 'site       build the website into site/dist'
	@echo 'site-serve build it, serve it, and rebuild on change'
	@echo 'shots      regenerate the terminal renders the site shows'
	@echo 'roadmap    regenerate ROADMAP.md from the backlog'
	@echo 'news       the release notes for $(VERSION), cut from NEWS'
	@echo 'dist       dirk-$(VERSION).tar.gz, buildable from source'
	@echo 'install    into $(PREFIX) — honours PREFIX and DESTDIR'
	@echo 'uninstall  take it back out again'
