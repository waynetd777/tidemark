# Disk Map

What is using the disk, and what grew.

<a href="images/index.md#disk-map"><picture><source media="(prefers-color-scheme: dark)" srcset="images/disk-map-dark.png"><img alt="The Disk Map screen" src="images/disk-map-light.png" width="900"></picture></a>

## The map

- Every folder is a block inside its parent, sized by **allocated bytes on disk** (not file lengths).
- A folder's loose files are one grey dashed block; folders too small to draw fold into **more folders**.
- Colour is nesting level, in shades of the tide colour.
- Click to zoom; **‹ ›** (⌘[ and ⌘]) go back and forward; ⌘-click reveals a block in <!-- mac -->Finder<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->File Explorer<!-- /windows --><!-- github --> (Windows)<!-- /github -->; right-click for Reveal in <!-- mac -->Finder<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->File Explorer<!-- /windows -->, Zoom in, Copy path and <!-- mac -->Get Info<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->Properties<!-- /windows -->.
- **Choose folder** in the sidebar (⌘O) scans a different folder; the five most recent sit below it. The default is the whole disk<!-- windows -->: on Windows the system drive, named as File Explorer names it (Windows (C:) or Local Disk (C:))<!-- /windows -->.
- A folder's own files are listed in a table you can sort and filter.

## Growth

- **Growth this week** outlines the folders where the disk's growth landed, with how much.
- It compares two scans at least 20 hours apart, so it shows from the day after the first scan. The **Growth this week** switch in the title bar hides it.

## Free space

- At the top of a whole-disk scan, free space is a dotted grey block beside the folders. The **Free space** switch in the title bar hides it.
- The sidebar's used figure is larger than the scan total: it includes <!-- mac -->snapshots, purgeable space, swap and folders the scan can't read<!-- /mac --><!-- github --> on a Mac, and on Windows <!-- /github --><!-- windows -->System Restore points and shadow copies, the file system's own records, and folders the scan can't read<!-- /windows -->.

## Cloud folders

- OneDrive, iCloud Drive and the like show two numbers: on disk, and in the cloud.<!-- windows --> On Windows, OneDrive, iCloud for Windows and other providers using Windows' cloud files are recognised by Windows' own marks on cloud files.<!-- /windows -->
- An online-only file takes no space. A cloud folder's block is the larger of its size on disk and 2% of its size in the cloud, so an online-only folder doesn't vanish.

## Scanning

- **Reload** (⌘R) scans again; while it runs it is **Stop**, which keeps the last finished scan on screen.
- A parallel walk in Rust across every core: about a minute for a million items on an SSD.
- <!-- mac -->It doesn't follow symlinks, counts hard links once, and skips /System/Volumes, /Volumes, /dev, /Network, /.vol, /cores and /private/var/vm.<!-- /mac --><!-- github --> On Windows: <!-- /github --><!-- windows -->Junctions and links are counted as themselves, never followed, and files hard-linked across the Windows folder (WinSxS and System32) are counted once, so the Windows folder isn't shown at nearly twice its size.<!-- /windows -->
- Each scan is cached and drawn at once on launch; a fresh scan runs only if the cached one is over an hour old, and never when Scan the disk is Off.
- Scheduled scans follow Settings. **Daily** waits until the last scan is 20 hours old and the computer has been idle for 5 minutes on power; once the last scan is two days old it runs when idle on battery too. **Hourly** scans every hour, idle or not, on battery or not. **Off** scans only when you press Reload.
- An automatic scan is skipped while another process (the app or the helper) is scanning; Reload always runs.

<!-- mac -->
## Full Disk Access

- Without it, Mail, Safari, Photos and other protected folders are skipped and counted, and a banner says so.
- Grant it under System Settings › Privacy & Security › Full Disk Access, then Reload.
<!-- /mac -->

<!-- windows -->
## Folders the scan can't read

- Windows has no Full Disk Access to give. The folders a scan can't read are mostly ones only an administrator can open: other accounts' folders, System Volume Information and parts of the Windows folder.
- They're skipped and counted; the sidebar's Scanning section shows how many.
<!-- /windows -->
