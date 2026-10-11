## Disk Map

What is using the disk: every folder as a block inside its parent, sized by what it takes up.

- The sections below (the map, getting around, files, free space, cloud folders, scanning) are all about this screen.
- **Growth this week** outlines the folders that grew this week, in black (white in dark mode), and tags each with how much.
- A folder is tagged where the growth landed: if most of ~/.colima's growth is in one folder inside it, that folder is tagged, not everything above it.
- Hover any folder to see how much it changed this week.
- Growth compares two scans at least 20 hours apart, so it shows from the day after the first scan. The switch hides it.

## The map

Each folder is a block with its name and size, where the block is big enough.

- Sizes are **allocated bytes on disk**, not file lengths: the question is what uses the disk.
- A folder's loose files are one grey dashed block, labelled with their count.
- Folders too small to draw fold into a **more folders** block, so a folder's area is always its total.
- **Colour is nesting level**: shades of the tide colour, deepest just below the root and lighter going in, then a greyer set from the fifth level. A folder keeps its colour as you zoom.
- Hover a block for its path, size, item count and share of its parent.

## Getting around

- **Click a folder** to zoom in. Clicking its loose-files block opens the same folder.
- The **breadcrumb** shows where you are; click a name to go back up. **Esc** or **⌘↑** go up one level.
- **‹ ›** (⌘[ and ⌘]) step back and forward.
- **Choose folder** in the sidebar (⌘O) scans a different folder. The five most recent sit below it; the default is the whole disk.

## Finder and the right-click menu

- **Reveal in Finder**: the arrow on a block's header, ⌘-click the block, or the arrow by the breadcrumb (⌘⇧R).
- **Right-click** a block for Reveal in Finder, Zoom in, Copy path and Get Info.

## Files in a folder

A folder's own files are listed in a Finder-style table: Name, Date Modified, Size, On Disk and Kind.

- It fills the map when a folder holds only files.
- Type in the box to filter by name (this folder only, not subfolders).
- Click a heading to sort; click again to reverse.
- Double-click a file, or press its arrow, to reveal it in Finder.
- Thousands of files load a page at a time as you scroll.
- **Size** is the file's length; **On Disk** is the space it takes. Listing never reads contents or downloads cloud files.

## Free space

The disk's free space, drawn beside the folders so the map shows the whole disk.

- It's a dotted grey block at the top of a whole-disk scan (the boot volume, or a disk under /Volumes), not inside a folder.
- The **Free space** switch in the title bar hides it; the choice is remembered.
- The sidebar's used figure is larger than the scan total. It includes Time Machine snapshots, purgeable space, swap, the recovery volume and folders the scan skips or can't read. Hover the bar for the breakdown.

## Cloud folders

OneDrive, iCloud Drive and anything under Library/CloudStorage or Library/Mobile Documents.

- They have a double edge and two numbers: what is on disk, and what is in the cloud.
- An online-only file takes no space, so it counts only in the "in cloud" figure.
- A cloud folder's block is the larger of its size on disk and 2% of its size in the cloud, so an online-only folder doesn't vanish; the tooltip says when a block is sized by the cloud.

## Scanning

- **Reload** (⌘R) scans again. While it runs the button is **Stop**, which keeps the last finished scan on screen.
- A scan walks every folder across every core: about a minute for a million items on an SSD.
- It doesn't follow symlinks, counts hard-linked files once, and skips /System/Volumes, /Volumes, /dev and the virtual folders.
- Each finished scan is cached and drawn at once on launch. A fresh scan runs only if the cached one is over an hour old, and never when Scan the disk is Off.
- The strip under the title bar shows what is being scanned and how far it has got.
- The disk is also scanned on Settings' schedule (daily by default), by the app or the background helper. A scan the helper finishes replaces the map on screen.

## Full Disk Access

Without it, macOS hides your protected folders (Mail, Safari, Photos and others) from the scan.

- Folders Tidemark can't read are skipped and counted, and a banner offers System Settings.
- Grant it under Privacy & Security › Full Disk Access, then press Reload.
- Some system folders owned by root (such as /private/var/folders) stay unreadable even then. The sidebar's Scanning section shows how many were skipped.
