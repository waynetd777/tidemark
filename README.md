# Tidemark

*The high-water marks of your Mac.*

A macOS app that records what is using your Mac's disk and how hard the Mac is working, now and over time, and marks the highs. Built with Tauri 2, React and Rust.

See it at [tidemark.davies.co.za](https://tidemark.davies.co.za).

<a href="docs/images/index.md#now"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/now-dark.png"><img alt="The Now screen" src="docs/images/now-light.png" width="900"></picture></a>

- **Now**: CPU, GPU, memory, disk I/O, network, energy and disk space at a glance, each with a meter, and the week's highs.
- **Recent**: every measure over 1 hour to 14 days (up to the history kept), a tab each with every part on its own chart, what each app was doing at any moment, incidents and crashes.
- **Trends**: every measure a day at a time over a month to a year, each day's busiest apps, the records, when the Mac is busiest, and when the disk would fill.
- **Disk Map**: every folder as a block sized by what it takes up, with this week's growth.
- **Menu bar** and a background helper keep it recording; **Ask** puts it all to an AI already signed in on your Mac.

## Install

Apple Silicon, macOS 13 or later.

1. Download [Tidemark.dmg](https://github.com/waynetd777/tidemark/releases/latest/download/Tidemark.dmg) from the [latest release](https://github.com/waynetd777/tidemark/releases/latest) and drag **Tidemark** onto **Applications**.
2. Open it. macOS warns because it is self-signed rather than notarised: click **Done**, then System Settings › Privacy & Security › **Open Anyway**, and **Open Anyway** again when macOS asks. Or run `xattr -dr com.apple.quarantine "/Applications/Tidemark.app"`.
3. Give it **Full Disk Access** (System Settings › Privacy & Security) so it can see inside Mail, Safari, Photos and the other protected folders.

Tidemark checks for a new version daily and asks before installing it; Tidemark › Check for Updates… checks now.

## Docs

| Page | About |
|---|---|
| [Features](docs/features.md) | Each screen, briefly, with its screenshot |
| [Recording](docs/recording.md) | Sampling, the history and what it keeps, the background helper, sleep and gaps, crashes |
| [Disk Map](docs/disk-map.md) | The map, growth, free space, cloud folders, scanning, Full Disk Access |
| [Development](docs/development.md) | Building, checks, screenshots, signing, where things live |

The app's own help (press `?`) covers every screen in detail.

## Licence

Free software under the GNU General Public License, version 3 or later. See [LICENSE](LICENSE).
