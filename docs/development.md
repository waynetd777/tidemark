# Development

Building, checking and changing Tidemark, on macOS and on Windows.

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

## Windows

Tidemark builds and runs on Windows 10 and 11 (64-bit) too. The Makefile's targets are macOS's; on Windows use npm and the Tauri CLI directly.

You need Rust with the MSVC toolchain, Node, Python 3, and Visual Studio Build Tools with the "Desktop development with C++" workload. WebView2 ships with Windows 10 and 11.

```powershell
git clone https://github.com/waynetd777/tidemark
cd tidemark
npm ci
npx tauri dev       # hot reload
npx tauri build     # the installer, in src-tauri/target/release/bundle/nsis
```

- The installer is NSIS, installed per user (`src-tauri/tauri.windows.conf.json`, with `src-tauri/windows/hooks.nsh`); updates install in passive mode, with a progress window.
- The Rust tests run on Windows with `cargo test --lib` in `src-tauri`. CI runs clippy and the tests on `windows-latest` for every push to main that touches the code (`.github/workflows/windows-tests.yml`), so code that only builds on macOS is caught on the push that adds it.
- Windows' code lives in `*_win.rs` modules beside the macOS ones: `system_win.rs` (GPU, disk I/O, memory split and pressure, cooling, core kinds, from the performance counters), `power_win.rs` (the battery), `crashes_win.rs` (the Application event log), `recorder_win.rs` (the background helper), `login_win.rs` (the Run key, for the helper and Open at Login), with `dirlist.rs` for the folder walk and `volume.rs` for paths and drives.
- Paths keep macOS's form inside the app, with `/` the system drive's root, and are shown in Windows' form (`C:\Users\…`) by `shown` in `src/util.ts`. The words and keys that differ (Finder or File Explorer, ⌘ or Ctrl) are in `src/keys.ts`.

### Writing for both platforms

The in-app help (`src/help/*.md`, also Ask's guide) and these docs are written once for both. Mark what only one platform has:

- `<!-- mac -->…<!-- /mac -->` for macOS only, `<!-- windows -->…<!-- /windows -->` for Windows only, and `<!-- github -->…<!-- /github -->` for the words that join the two on GitHub, which shows both; the help never shows them. For example `<!-- mac -->Finder<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->File Explorer<!-- /windows --><!-- github --> (Windows)<!-- /github -->`.
- Markers are written exactly so, don't nest, and each opener has its closer. `forPlatform` in `src/guides.ts` (and `src-tauri/src/assistant/mod.rs` for Ask's copy) drops the github parts, then the other platform's, then the markers, and squeezes blank lines. To keep a list or a table whole, put the opener at the end of the line before a platform's bullet or row and the closer at the end of that bullet or row, not on lines of their own, which break a table on GitHub.
- Write shortcuts as macOS does (⌘1, ⌘⇧R, ⌘-click): the app shows Ctrl on Windows. Prefer neutral words ("your computer", "the disk") to markers where they read naturally.
- A `##` heading is a section the help opens by title (`FOR_SCREEN` in `src/Help.tsx`). A heading that differs by platform needs both titles there.

## Where things live

| What | Where |
|---|---|
| Scans, settings, the history, the helper's log (`helper.log`) | `~/Library/Application Support/Tidemark/`, or on Windows `%LOCALAPPDATA%\com.wayned.tidemark\` (see [Recording](recording.md#the-history)) |
| Ask's working folders, one per AI tool | `~/Library/Application Support/com.wayned.tidemark/`, or on Windows `%APPDATA%\com.wayned.tidemark\` |
| Scanning and disk commands | `src-tauri/src/scan.rs`, `disk.rs`, `cache.rs` |
| The recorder and incidents | `src-tauri/src/monitor.rs` |
| Energy, GPU and memory, crashes | `src-tauri/src/energy.rs`, `system.rs`, `crashes.rs`; on Windows `power_win.rs`, `system_win.rs`, `crashes_win.rs` |
| History and trends | `src-tauri/src/history.rs`, `trends.rs` |
| Schedule, the background helper, Open at Login | `src-tauri/src/schedule.rs`, `recorder.rs`, `login_item.rs`, `login_launch.rs`; on Windows `recorder_win.rs`, `login_win.rs` |
| Updates | `src-tauri/src/updates.rs`, its dialog `src/Update.tsx`, `tools/update_files.py` |
| The menu-bar window (the notification area's on Windows) | `src-tauri/src/tray.rs`, `src/Tray.tsx` |
| The window, sidebar and shortcuts | `src/App.tsx` |
| Screens and charts | `src/Now.tsx`, `Performance.tsx` (Recent), `Trends.tsx`, `Settings.tsx`, `charts.tsx`, `status.ts` |
| Ask | `src/Ask.tsx`, `askDetails.ts`, `src-tauri/src/assistant/` |
| The map and file table | `src/Treemap.tsx`, `src/FileTable.tsx` |
| In-app help | `src/help/*.md`, loaded by `src/guides.ts`, which keeps this platform's parts |
| Platform words and keys, paths shown Windows' way | `src/keys.ts`, `src/util.ts` (`shown`), `src-tauri/src/volume.rs` |
| Installer and icons | `tools/dmg/`, `tools/make_icons.py`, `design/icon.png`; on Windows `src-tauri/tauri.windows.conf.json`, `src-tauri/windows/` |
