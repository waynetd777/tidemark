# Tidemark — build, sign and install the app.

APP      := src-tauri/target/release/bundle/macos/Tidemark.app

# The signing certificate is named in signing.local, which is untracked: the name of a
# keychain identity is local to the machine that holds it. Copy signing.local.example and
# put your own self-signed certificate's name in it. See docs/development.md.
-include signing.local
export APPLE_SIGNING_IDENTITY
SIGN_ID  := $(APPLE_SIGNING_IDENTITY)


.PHONY: check test lint fmt app install-app dmg dev icons screenshots sign-check

## cargo test + TypeScript type-check + lint.
check: lint
	cd src-tauri && cargo test --lib
	npx tsc --noEmit -p tsconfig.json

## Every language in the repo, failing on any warning: rustfmt and clippy for the Rust; ESLint
## and Prettier for the TypeScript, CSS, JSON and HTML; ruff for the Python in tools/; shellcheck
## for the shell scripts and git hooks.
lint:
	cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
	npm run lint
	ruff check tools && ruff format --check tools
	shellcheck $(wildcard tools/*.sh) .githooks/*

## Reformat everything in place (rustfmt, Prettier, ruff).
fmt:
	cd src-tauri && cargo fmt
	npx prettier --write .
	ruff format tools
	ruff check --fix tools

test: check

# Release builds strip the builder's home directory out of the binary. Rust bakes absolute
# paths (~/.cargo/registry, ~/.rustup) into panic metadata, which would otherwise ship the
# builder's username to everyone who downloads the app. Debug builds skip this so `make dev`
# and `make check` keep their incremental caches.
RELEASE_RUSTFLAGS := --remap-path-prefix=$(HOME)=/build

## Build the signed .app. The identity comes from signing.local; see docs/development.md for why a
## stable signature matters (Full Disk Access is tied to it).
app:
	@test -n "$(SIGN_ID)" || { echo "APPLE_SIGNING_IDENTITY is not set: copy signing.local.example to signing.local"; exit 1; }
	RUSTFLAGS="$(RELEASE_RUSTFLAGS)" npm run tauri build
	@codesign -dv --verbose=2 "$(APP)" 2>&1 | grep -E "^Authority=$(SIGN_ID)" >/dev/null \
	  && echo "signed with $(SIGN_ID)" \
	  || { echo "WARNING: app is not signed with $(SIGN_ID); privacy grants will not persist"; exit 1; }

## Build and replace /Applications/Tidemark.app (same signature, so Full Disk Access persists).
## The background helper runs the same binary: its launch agent is unloaded first (so launchd
## neither restarts it from a half-copied app nor throttles the restart) and loaded again after,
## if it was loaded, even when the copy fails (the target still fails). The app is asked to quit
## (an Apple event, so it saves the minute in hand and ends open incidents as it does on ⌘Q),
## with pkill only for one that hasn't gone within 5 seconds.
HELPER_AGENT := gui/$(shell id -u)/com.wayned.tidemark.helper
HELPER_PLIST := $(HOME)/Library/LaunchAgents/com.wayned.tidemark.helper.plist
install-app: app
	@loaded=; ok=1; \
	if launchctl print "$(HELPER_AGENT)" >/dev/null 2>&1; then \
	  loaded=1; launchctl bootout "$(HELPER_AGENT)" 2>/dev/null || true; \
	fi; \
	if pgrep -x Tidemark >/dev/null; then \
	  osascript -e 'quit app id "com.wayned.tidemark"' >/dev/null 2>&1 || true; \
	  for _ in 1 2 3 4 5 6 7 8 9 10; do pgrep -x Tidemark >/dev/null || break; sleep 0.5; done; \
	fi; \
	pkill -x Tidemark 2>/dev/null || true; \
	{ rm -rf "/Applications/Tidemark.app" && \
	  ditto "$(APP)" "/Applications/Tidemark.app" && \
	  echo "installed /Applications/Tidemark.app"; } || ok=; \
	if [ -n "$$loaded" ]; then \
	  if launchctl bootstrap "gui/$$(id -u)" "$(HELPER_PLIST)"; then \
	    echo "restarted the background helper"; \
	  else ok=; fi; \
	fi; \
	test -n "$$ok"

## Build the app and pack it into dist-release/Tidemark.dmg for a GitHub release, laid out
## like other Mac installers (tools/dmg/). Finder lays out the window, so the first run asks to
## let the terminal control Finder.
dmg: app
	@python3 tools/dmg/make_dmg.py

## Redraw design/icon.png and the tray template, then regenerate the Tauri icon set.
icons:
	@python3 tools/make_icons.py
	@npx tauri icon design/icon.png >/dev/null
	@rm -rf src-tauri/icons/android src-tauri/icons/ios
	@cp src-tauri/icons/128x128@2x.png public/icon.png
	@echo "regenerated src-tauri/icons and public/icon.png"

## Retake docs/images/<name>-light.png and -dark.png from demo data (see docs/images/index.md).
screenshots:
	@python3 tools/screenshots.py

sign-check:
	@codesign -dv --verbose=2 "/Applications/Tidemark.app" 2>&1 | grep -E "^(Identifier|Authority|Signature|TeamIdentifier)"

dev:
	npm run tauri dev
