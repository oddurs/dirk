# dirk — convenience targets. The build itself is cargo's job; this exists for
# what cargo does not cover: the checks CI runs, and installing the binary
# where a system expects to find it.
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
CARGO  ?= cargo
DIRK   := target/release/dirk

.PHONY: all build check test fmt lint shot roadmap install clean

all: build

build:
	$(CARGO) build --release

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

# Print what dirk currently paints, as plain text. The fastest way to see
# whether a change to the chrome did what you meant.
shot:
	$(CARGO) run --example shot

# Regenerate the roadmap from the backlog.
roadmap:
	cairn render

install: build
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 $(DIRK) $(DESTDIR)$(BINDIR)/dirk

clean:
	$(CARGO) clean
