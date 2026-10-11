# Development

Building, checking and changing Tidemark.

## Build

You need Rust, Node, Python 3 with Pillow (for the screenshots and icons), ruff and shellcheck (for `make lint`), and a self-signed code-signing certificate for `make install-app` (see Signing and releasing, below).

```bash
git clone https://github.com/waynetd777/tidemark ~/Projects/tidemark
cd ~/Projects/tidemark
npm install
git config core.hooksPath .githooks   # the lint hook
make dev              # hot reload
cp signing.local.example signing.local   # then set your certificate's name in it
make install-app      # signed build, copied to /Applications
```

## Commands

| Command | What it does |
|---|---|
| `make dev` | Run with hot reload (Open at Login and the helper need the installed app) |
| `make install-app` | Build the signed app and replace the copy in /Applications. It quits a running Tidemark without reopening it, and stops and restarts the background helper if it is loaded |
| `make dmg` | Build the installer, `dist-release/Tidemark.dmg`, and the updater's files beside it (`Tidemark.app.tar.gz`, its `.sig`, `latest.json`). The first run asks to let the terminal control Finder, which lays out the window |
| `make release NOTES="…"` | Publish a release: bump, `make dmg`, commit, push, GitHub release, install (`tools/release.py`) |
| `make check` | Lint, Rust tests and TypeScript type-check |
| `make lint` | rustfmt, clippy, ESLint, Prettier, ruff and shellcheck; fails on any warning |
| `make fmt` | Reformat everything in place |
| `make icons` | Redraw the icon artwork and regenerate the icon set |
| `make screenshots` | Retake the screenshots in `docs/images/` and the site's `site/img/*.webp` from demo data. It runs the debug build, so run `make dev` first to bring it up to date; needs Pillow |

- The pre-commit hook runs `make lint` for any commit that touches code.
- Screenshots come from demo data, never this Mac's: see [images/index.md](images/index.md).
- The docs show each screenshot in the reader's theme with a `<picture>` element, linked to [images/index.md](images/index.md), which lists them all in light and dark. A new screenshot goes in that list too.

## Signing and releasing

- macOS ties privacy grants to the signature, so builds use a self-signed certificate named in `signing.local` (copy `signing.local.example`).
- To release: `make release NOTES="What's new"` (`tools/release.py`). It refuses to start without notes or with uncommitted changes, then moves to the next patch version once the current one has been released, that is, once `v<version>` is tagged on GitHub (`tools/bump_version.py`, which keeps `tauri.conf.json`, `package.json`, `Cargo.toml` and the lock files in step; `VERSION=2.2.0` sets it), runs `make dmg` (the notes go in the update offer), commits the bump as `Tidemark <version>`, pushes, creates the GitHub release with `Tidemark.dmg`, `Tidemark.app.tar.gz` and `latest.json`, and installs the app, opening it again if it was running. `INSTALL=0` skips the install, `DRY_RUN=1` prints the steps. After a failure, run it again: the unreleased version is kept.
- Updates: the installed app checks the latest release's `latest.json` 20 seconds after launch and then daily (`src-tauri/src/updates.rs`, endpoint and public key in `tauri.conf.json` under `plugins.updater`), and installs only a package signed with the matching private key, `~/.tauri/waynetd777-updater.key`, which `tools/update_files.py` signs with. The key is not in the repo: keep a copy somewhere safe, because without it no installed copy will accept another update. Installing stops the background helper while the app is replaced and starts it again on the new build, as `make install-app` does. `python3 tools/update_files.py --notes "…"` adds notes to the update prompt.
- `TIDEMARK_TIMING=1 "/Applications/Tidemark.app/Contents/MacOS/Tidemark"` prints startup timings.

## Where things live

| What | Where |
|---|---|
| Scans, settings, the history, the helper's log (`helper.log`) | `~/Library/Application Support/Tidemark/` (see [Recording](recording.md#the-history)) |
| Ask's working folders, one per AI tool | `~/Library/Application Support/com.wayned.tidemark/` |
| Scanning and disk commands | `src-tauri/src/scan.rs`, `disk.rs`, `cache.rs` |
| The recorder and incidents | `src-tauri/src/monitor.rs` |
| Energy, GPU and memory, crashes | `src-tauri/src/energy.rs`, `system.rs`, `crashes.rs` |
| History and trends | `src-tauri/src/history.rs`, `trends.rs` |
| Schedule, the background helper, Open at Login | `src-tauri/src/schedule.rs`, `recorder.rs`, `login_item.rs`, `login_launch.rs` |
| Updates | `src-tauri/src/updates.rs`, its dialog `src/Update.tsx`, `tools/update_files.py` |
| The menu-bar window | `src-tauri/src/tray.rs`, `src/Tray.tsx` |
| The window, sidebar and shortcuts | `src/App.tsx` |
| Screens and charts | `src/Now.tsx`, `Performance.tsx` (Recent), `Trends.tsx`, `Settings.tsx`, `charts.tsx`, `status.ts` |
| Ask | `src/Ask.tsx`, `askDetails.ts`, `src-tauri/src/assistant/` |
| The map and file table | `src/Treemap.tsx`, `src/FileTable.tsx` |
| In-app help | `src/help/*.md`, loaded by `src/guides.ts` |
| Installer and icons | `tools/dmg/`, `tools/make_icons.py`, `design/icon.png` |
