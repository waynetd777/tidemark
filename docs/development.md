# Development

Building, checking and changing Tidemark.

## Build

```bash
git clone https://github.com/waynetd777/tidemark ~/Projects/tidemark
cd ~/Projects/tidemark
npm install
make dev              # hot reload
make install-app      # signed build, copied to /Applications
```

## Commands

| Command | What it does |
|---|---|
| `make dev` | Run with hot reload (Open at Login and the helper need the installed app) |
| `make install-app` | Build the signed app and replace the copy in /Applications |
| `make dmg` | Build the installer, `dist-release/Tidemark.dmg` |
| `make check` | Lint, Rust tests and TypeScript type-check |
| `make lint` | rustfmt, clippy, ESLint, Prettier, ruff, shellcheck and plutil; fails on any warning |
| `make fmt` | Reformat everything in place |
| `make icons` | Redraw the icon artwork and regenerate the icon set |
| `make screenshots` | Retake the screenshots in `docs/images/` from demo data |

- The pre-commit hook runs `make lint` for any commit that touches code.
- Screenshots come from demo data, never this Mac's: see [images/index.md](images/index.md).
- The docs show each screenshot in the reader's theme with a `<picture>` element, linked to [images/index.md](images/index.md), which lists them all in light and dark. A new screenshot goes in that list too.

## Signing and releasing

- macOS ties privacy grants to the signature, so builds use a self-signed certificate named in `signing.local` (copy `signing.local.example`).
- To release: bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`, run `make dmg`, push, then `gh release create v<version> dist-release/Tidemark.dmg`.
- `TIDEMARK_TIMING=1 "/Applications/Tidemark.app/Contents/MacOS/Tidemark"` prints startup timings.

## Where things live

| What | Where |
|---|---|
| Scans, settings, the history | `~/Library/Application Support/Tidemark/` |
| Scanning and disk commands | `src-tauri/src/scan.rs`, `disk.rs`, `cache.rs` |
| The recorder and incidents | `src-tauri/src/monitor.rs` |
| Energy, GPU and memory, crashes | `src-tauri/src/energy.rs`, `system.rs`, `crashes.rs` |
| History and trends | `src-tauri/src/history.rs`, `trends.rs` |
| Schedule and the background helper | `src-tauri/src/schedule.rs`, `recorder.rs`, `src-tauri/launchagent/` |
| The menu-bar window | `src-tauri/src/tray.rs`, `src/Tray.tsx` |
| Screens and charts | `src/Now.tsx`, `Performance.tsx`, `Trends.tsx`, `Settings.tsx`, `charts.tsx`, `status.ts` |
| The map and file table | `src/Treemap.tsx`, `src/FileTable.tsx` |
| In-app help | `src/help/*.md`, loaded by `src/guides.ts` |
| Installer and icons | `tools/dmg/`, `tools/make_icons.py`, `design/icon.png` |
