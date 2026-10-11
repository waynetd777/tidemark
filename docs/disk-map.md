# Disk Map

What is using the disk, and what grew.

<a href="images/index.md#disk-map"><picture><source media="(prefers-color-scheme: dark)" srcset="images/disk-map-dark.png"><img alt="The Disk Map screen" src="images/disk-map-light.png" width="900"></picture></a>

## The map

- Every folder is a block inside its parent, sized by **allocated bytes on disk** (not file lengths).
- A folder's loose files are one grey dashed block; folders too small to draw fold into **more folders**.
- Colour is nesting level, in shades of the tide colour.
- Click to zoom; **‹ ›** (⌘[ and ⌘]) go back and forward; ⌘-click reveals a block in Finder; right-click for Reveal in Finder, Zoom in, Copy path and Get Info.
- **Choose folder** in the sidebar (⌘O) scans a different folder; the five most recent sit below it.
- A folder's own files are listed in a Finder-style table you can sort and filter.

## Growth

- **Growth this week** outlines the folders where the disk's growth landed, with how much.
- It compares two scans at least 20 hours apart, so it shows from the day after the first scan. The **Growth this week** switch in the title bar hides it.

## Free space

- At the top of a whole-disk scan, free space is a dotted grey block beside the folders. The **Free space** switch in the title bar hides it.
- The sidebar's used figure is larger than the scan total: it includes snapshots, purgeable space, swap and folders the scan can't read.

## Cloud folders

- OneDrive, iCloud Drive and the like show two numbers: on disk, and in the cloud.
- An online-only file takes no space. A cloud folder's block is the larger of its size on disk and 2% of its size in the cloud, so an online-only folder doesn't vanish.

## Scanning

- **Reload** (⌘R) scans again; while it runs it is **Stop**, which keeps the last finished scan on screen.
- A parallel walk in Rust across every core: about a minute for a million items on an SSD.
- It doesn't follow symlinks, counts hard links once, and skips /System/Volumes, /Volumes, /dev, /Network, /.vol, /cores and /private/var/vm.
- Each scan is cached and drawn at once on launch; a fresh scan runs only if the cached one is over an hour old, and never when Scan the disk is Off.
- Scheduled scans follow Settings. **Daily** waits until the last scan is 20 hours old and the Mac has been idle for 5 minutes on power; once the last scan is two days old it runs when idle on battery too. **Hourly** scans every hour, idle or not, on battery or not. **Off** scans only when you press Reload.
- An automatic scan is skipped while another process (the app or the helper) is scanning; Reload always runs.

## Full Disk Access

- Without it, Mail, Safari, Photos and other protected folders are skipped and counted, and a banner says so.
- Grant it under System Settings › Privacy & Security › Full Disk Access, then Reload.
