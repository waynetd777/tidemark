# Tidemark

*The high-water marks of your Mac.*

A small macOS app that records what is using your Mac's disk and how hard the Mac is working,
now and over time, and marks the highs: the tidemarks.

Built with Tauri 2, React and Rust.

![The disk map, showing the whole disk with its free space](docs/screenshots/app.png)

## What it does

The user guide is [docs/guide.md](docs/guide.md); it is also the app's help (press `?`, or Help ›
Tidemark Help). Four screens, in the sidebar:

- **Now** (⌘1): how the Mac is doing in a sentence; CPU, GPU, memory, disk I/O, network and energy over the last
  hour and disk space over the week, each with its tidemark (the week's high); the week's
  peaks and incidents; and what grew on the disk.
- **Performance** (⌘2): CPU and memory over 1 hour to 14 days, as minute averages with each
  minute's peak as a band behind; network download and upload; energy (the power the Mac draws,
  battery charge, health and cycles, what the last sleep cost, and each app's share); swap, disk
  reads and writes and thermal state. Click any moment for a snapshot of what every app was doing.
  **Incidents** are recorded when CPU stays above 90% for 30 seconds, memory pressure goes red or
  swap grows by 1 GB in 10 minutes, each with a snapshot.
- **Disk** (⌘3): every folder as a block inside its parent's, sized by what it takes up. Colour is
  nesting level; a folder's loose files are one grey dashed block; folders too small to draw fold
  into a "more folders" block; the disk's free space is a dotted block beside the folders; cloud
  folders (OneDrive, iCloud Drive) show what is on disk and what is in the cloud. Click to zoom,
  ‹ › to go back and forward, a Finder-style table for the files in a folder, Reveal in Finder and
  Get Info from a right-click. **Growth this week** outlines the folders where the disk's growth
  landed.
- **Trends** (⌘4): the disk over 14 days to 12 months, with its high-water mark, its biggest rises
  and when it would be full at that rate; the folders that rose; and CPU by weekday and hour.

Every chart runs up to now (a dotted line), with a legend, and shades its gaps: **Asleep** where
the Mac slept, hatched **Not recording** where Tidemark was quit or paused.

- **Menu bar**: closing the window keeps Tidemark recording there. Its icon is marked when
  something needs attention (memory, CPU, swap, a low battery, a nearly full disk); clicking it
  opens a small window with the live readings, what is using the most, and the action that fixes
  what needs attention.
- **Ask** (⌘K) puts what Tidemark knows (the folder on screen, what grew, the performance record)
  to an AI command-line tool already signed in on the Mac (Claude Code, Codex, Antigravity or
  GitHub Copilot; no key is set up in the app). Ask why the Mac was slow on Wednesday, or where
  space can be freed safely. The AI sees names, sizes and figures only, and the app changes nothing.
- **Settings** (⌘,): keep recording when quit (a background helper), Open at Login, how many days
  of history to keep, when to scan the disk, and the appearance.

## Recording

Tidemark samples the Mac every 2 seconds while its window is in front or the menu-bar window is
open; otherwise every 5 seconds, reading each app only every 15. It keeps one row a minute: the
average and the peak, with the busiest apps of that minute. Power and battery come from the
battery's controller, read every 10 seconds; each app's energy is the kernel's count for its
processes.

Everything goes in one SQLite file, `~/Library/Application Support/Tidemark/history.sqlite`: the
minutes, incidents, sleeps, the disk's used space hourly and the big folders of each scan for 14
days at most (Settings), and a row a day of disk totals and top folders for a year.

It records while it is running. **Keep recording when quit** (Settings) registers a background
helper, the app's own binary run as `Tidemark --record` by a launch agent (`SMAppService`; the plist
is bundled in `Contents/Library/LaunchAgents`). The app and the helper share a lock: whichever holds
it records, and the other shows what it shares in `live.json`, taking over if it stops. The helper
also runs the scheduled disk scans.

## Scanning

The scan is a parallel walk in Rust (one task per folder, work-stealing across every core). A full
boot volume of about a million items takes on the order of half a minute on an SSD. It does not
follow symlinks and does not descend into `/System/Volumes` (where the Data volume really lives,
so walking it from `/` would count everything twice), `/Volumes`, `/dev` or the virtual folders.

Sizes are **allocated bytes on disk**, not file lengths, because the question is what is using the
disk. A cloud file that is online-only has a length but no blocks, so it counts as nothing on disk
and shows up only in the folder's "in cloud" figure. So that an online-only OneDrive does not
vanish, a cloud folder's block is never drawn smaller than 2% of its cloud size; inside such a
folder the blocks are then proportional to cloud size, and the tooltip says so.

Every completed scan is **cached** under `~/Library/Application Support/Tidemark/`,
one file per root. On launch the cached map is drawn at once, and a fresh scan runs behind it if the
cached one is more than an hour old; the strip under the title bar shows the folder being walked,
the items counted so far, and a percentage measured against the previous scan's count. Scheduled
scans of the disk run daily (when the Mac is idle and on power), hourly or not at all, as Settings
says; each one's big folders go into the history, which is how growth is known.

The scan total is smaller than the volume's "used" figure in the sidebar: that figure comes from
the filesystem and includes local Time Machine snapshots, purgeable space and folders the walk
skips or cannot read.

The file table reads current file metadata when opened, so it can differ from the last scanned
totals in the map. **Size** is the file's length; **On Disk** is its allocated disk space. Listing
files does not read their contents or download online-only cloud files.

Folders the app is not allowed to read are skipped and counted. If any were skipped and the app
does not have Full Disk Access, a banner says so and opens System Settings.

## Install

Apple Silicon Macs, macOS 13 or later. No need to clone anything.

1. **Download** [Tidemark.dmg](https://github.com/waynetd777/tidemark/releases/latest/download/Tidemark.dmg)
   from the [latest release](https://github.com/waynetd777/tidemark/releases/latest).
2. **Open it** and drag **Tidemark** onto **Applications**.
3. **Open Tidemark** from Applications. macOS says it can't check the app for malicious
   software, because it is signed with a self-signed certificate rather than an Apple Developer ID
   and isn't notarised. Click **Done**, then open System Settings → Privacy & Security, scroll down
   and click **Open Anyway** beside Tidemark, then **Open Anyway** again and enter your password.
   It opens normally after that.
4. **Give it Full Disk Access** under System Settings → Privacy & Security → Full Disk Access, so
   it can see inside Mail, Safari, Photos and the other protected folders. Without it those folders
   are skipped and a banner in the app says so.

Closing the window hides it; the app stays in the menu bar, and Quit is in its menu-bar window. To
keep recording while it is quit, turn on **Keep recording when quit** in Settings; macOS may ask you
to allow it under System Settings → General → Login Items.

Instead of step 3 you can clear the download flag in Terminal, which is all Open Anyway does:

```bash
xattr -dr com.apple.quarantine "/Applications/Tidemark.app"
```

### Build it yourself

```bash
git clone https://github.com/waynetd777/tidemark ~/Projects/tidemark
cd ~/Projects/tidemark
npm install
make install-app      # signed build, copied to /Applications
```

## Commands

| Command | What it does |
|---|---|
| `make install-app` | Build the signed app and replace the copy in /Applications |
| `make dmg` | Build the app and pack it into the installer, `dist-release/Tidemark.dmg` |
| `make check` | Lint, Rust tests and TypeScript type-check |
| `make lint` | Every language: rustfmt and clippy (Rust), ESLint and Prettier (TypeScript, CSS, HTML), ruff (Python), shellcheck (scripts, hooks), plutil (plist); fails on any warning. The pre-commit hook runs it |
| `make fmt` | Reformat the Rust, TypeScript, CSS, HTML and Python in place |
| `make dev` | App with hot reload (Open at Login and the background helper need the installed app) |
| `make icons` | Redraw the icon artwork (`tools/make_icons.py`) and regenerate the Tauri icon set |
| `make screenshots` | Recapture `docs/screenshots/app.png` (needs Screen Recording for the terminal) |

## Notes

**Code signing.** macOS ties privacy grants to an app's signature, so an unsigned build would need
Full Disk Access granted again after every rebuild. Builds are signed with a self-signed
certificate in the login keychain rather than an Apple Developer ID. Copy `signing.local.example`
to `signing.local` (untracked) and name your own certificate there; `make app` refuses to build
without it. That is why a downloaded copy needs Open Anyway (or `xattr -dr com.apple.quarantine`)
once; a build made on the signing machine runs without it.

**Releasing.** Bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and
`package.json`, run `make dmg`, commit and push, then
`gh release create v<version> dist-release/Tidemark.dmg`. The download link above points at the
latest release's `Tidemark.dmg`, so keep that name. `tools/dmg/make_dmg.py` renders the window's
background from `tools/dmg/background.html` with WebKit and has Finder lay out the icons, so the
first run asks to let the terminal control Finder.

**Build times.** `src-tauri/Cargo.toml` keeps the lib `rlib`-only and uses thin LTO with parallel
codegen units, which takes an incremental build from minutes to about twenty seconds at the cost of
about a megabyte of binary.

**Startup.** The window appears already painted in the theme colour (or the one Settings chose),
with an inline splash until the cached scan is read. To see the timings:

```bash
TIDEMARK_TIMING=1 "/Applications/Tidemark.app/Contents/MacOS/Tidemark"
```

**Where things live**

| What | Where |
|---|---|
| Cached scans, recent roots, settings, the history | `~/Library/Application Support/Tidemark/` |
| The scanner, and the disk commands | `src-tauri/src/scan.rs`, `src-tauri/src/disk.rs` |
| The recorder and its incidents | `src-tauri/src/monitor.rs` |
| Energy and the battery | `src-tauri/src/energy.rs` |
| The history and its queries | `src-tauri/src/history.rs`, `src-tauri/src/trends.rs` |
| Scheduled scans and upkeep | `src-tauri/src/schedule.rs` |
| The background helper | `src-tauri/src/recorder.rs`, `src-tauri/launchagent/` |
| The menu-bar window | `src-tauri/src/tray.rs`, `src/Tray.tsx` |
| The screens | `src/Now.tsx`, `src/Performance.tsx`, `src/Trends.tsx`, `src/Settings.tsx` |
| The charts | `src/charts.tsx` |
| The treemap | `src/Treemap.tsx` |
| The searchable file table | `src/FileTable.tsx` |
| File metadata listing | `src-tauri/src/files.rs` |
| Installer layout | `tools/dmg/` (`background.html` is the window's background) |
| Icon artwork | `design/icon.png`, `src-tauri/icons/tray@2x.png` and `tray-alert@2x.png`, drawn by `tools/make_icons.py` |

## Licence

MIT. See [LICENSE](LICENSE). Use the code for anything, including commercially,
as long as the copyright notice and licence text come along with it.
