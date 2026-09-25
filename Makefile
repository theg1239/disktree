# disktree: build, check, and install.
#
# `make install` puts the binary, a desktop entry and an icon under PREFIX
# (default: ~/.local), so disktree shows up in the Omarchy launcher and in
# "Open with" for directories. The default needs no root:
#
#   make install                         # ~/.local/bin/disktree
#   sudo make install PREFIX=/usr/local  # system-wide

PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
APPDIR ?= $(PREFIX)/share/applications
ICONDIR ?= $(PREFIX)/share/icons/hicolor/scalable/apps

MANIFEST = Cargo.toml
CARGO ?= cargo
CARGO_BUILD_FLAGS ?=
TARGET = target/release/disktree
BUNDLE ?= target/release/Disktree.app
MAC_APPDIR ?= $(HOME)/Applications
ifeq ($(shell uname -s),Darwin)
export MACOSX_DEPLOYMENT_TARGET = 12.0
# macOS 27 rejects host proc-macros misaligned by LLVM's debug stripper.
# Scope the workaround to Mac builds; keep the workspace profiles unchanged.
# https://github.com/rust-lang/rust/issues/157750
CARGO_BUILD_FLAGS += --config 'profile.release.build-override.strip="none"'
CARGO_BUILD_FLAGS += --config 'profile.release.package.gpui-pre-macros.strip="none"'
endif
ICON = assets/disktree.svg
DESKTOP = packaging/disktree.desktop.in

.PHONY: help build run bundle install uninstall lint test ci fmt clean

help:
	@echo "disktree"
	@echo
	@echo "  make build       release build"
	@echo "  make bundle      macOS .app bundle"
	@echo "  make run         build and run, scanning $$HOME"
	@echo "  make install     macOS: ~/Applications; Linux: $(PREFIX)"
	@echo "  make uninstall   remove what install put there"
	@echo "  make lint        rustfmt --check and clippy -D warnings"
	@echo "  make test        core and window-harness tests"
	@echo "  make ci          lint, then test"
	@echo "  make fmt         format in place"
	@echo "  make clean       cargo clean"

# Always ask cargo: it is incremental and knows every source file, where a
# make file-target would only compare the binary against the manifest and
# happily install a stale build.
build:
	$(CARGO) build --release $(CARGO_BUILD_FLAGS)

run: build
	$(TARGET)

lint:
	$(CARGO) xtask lint

test:
	$(CARGO) xtask test

ci: lint test

fmt:
	$(CARGO) xtask fmt-fix

ifeq ($(shell uname -s),Darwin)
bundle: build
	packaging/macos/bundle.sh "$(TARGET)" "$(BUNDLE)"

install: bundle
	mkdir -p "$(MAC_APPDIR)"
	ditto "$(BUNDLE)" "$(MAC_APPDIR)/Disktree.app"
	@echo "Installed $(MAC_APPDIR)/Disktree.app"

uninstall:
	rm -rf "$(MAC_APPDIR)/Disktree.app"
else
bundle:
	@echo "App bundles require macOS" >&2
	@exit 1

install: build
	install -d $(BINDIR) $(APPDIR) $(ICONDIR)
	install -m755 $(TARGET) $(BINDIR)/disktree
	install -m644 $(ICON) $(ICONDIR)/disktree.svg
	VERSION=$$(sed -n 's/^version = "\(.*\)"/\1/p' $(MANIFEST) | head -1) && \
	sed -e 's|@BINDIR@|$(BINDIR)|' -e "s|@VERSION@|$$VERSION|" \
	    $(DESKTOP) > $(APPDIR)/disktree.desktop && \
	chmod 644 $(APPDIR)/disktree.desktop
	@if command -v update-desktop-database >/dev/null 2>&1; then \
	    update-desktop-database $(APPDIR) 2>/dev/null || true; \
	fi
	@echo
	@echo "installed:"
	@echo "  $(BINDIR)/disktree"
	@echo "  $(APPDIR)/disktree.desktop"
	@echo "  $(ICONDIR)/disktree.svg"
	@if command -v desktop-file-validate >/dev/null 2>&1; then \
	    desktop-file-validate $(APPDIR)/disktree.desktop || true; \
	fi
	@case ":$$PATH:" in *":$(BINDIR):"*) ;; *) \
	    echo; echo "note: $(BINDIR) is not on PATH in this shell";; esac

uninstall:
	rm -f $(BINDIR)/disktree $(APPDIR)/disktree.desktop $(ICONDIR)/disktree.svg
	@if command -v update-desktop-database >/dev/null 2>&1; then \
	    update-desktop-database $(APPDIR) 2>/dev/null || true; \
	fi
	@echo "removed"

endif

clean:
	$(CARGO) clean
