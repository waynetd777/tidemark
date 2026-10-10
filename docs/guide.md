# Tidemark

Tidemark records the high-water marks of your Mac: what is using the disk and how hard the Mac is working, now and over time. It has four screens, in the sidebar: **Now** (⌘1), **Performance** (⌘2), **Disk** (⌘3) and **Trends** (⌘4), with **Settings** (⌘,) at the foot. Tidemark opens on the screen you last had open.

The ‹ › buttons at the top left of every screen (⌘[ and ⌘]) step back and forward through where you have been, across screens and folders, like Finder.

This guide is also the app's help: press `?`, the ? button in the title bar, or Help › Tidemark Help, and it opens on the sections for the screen you are on; **The whole guide** at its foot shows every section, and typing searches all of them. Help › Tidemark Website opens tidemark.davies.co.za.

## Now

How the Mac is doing, in a sentence, and the things that matter most: CPU, GPU (where the Mac reports it), memory, network and energy across the top, over the last 24 hours, and disk space over the last week beside **This week's tidemarks** (the week's peaks and incidents, see Performance) and **What grew on disk this week** (the folders where the disk's growth landed). Each card's **Details** opens the screen with more.

Questions at the foot open Ask with that question typed in.

## Reading the charts

Every chart in Tidemark reads the same way, and its legend says what each mark is.

- The **line** is the measure's average over each point: a minute on the 1-hour range, a few minutes on longer ones (the legend says how many), and on Trends the disk's used space. The newest point is the live reading, so the line reaches now.
- On Performance, a faint **band** behind the line is the busiest moment within each point, so a spike of a few seconds still shows.
- The dashed line across is the **tidemark**: the highest that measure has been this week (on Trends, in the range). On Performance a dot and a label mark the range's peak.
- A dotted line at the right-hand end is **now**; the line runs up to the latest reading.
- **Gaps** are shaded for what they were: a soft band marked Asleep, or a hatched band marked Not recording (Tidemark was quit or paused, or had not started yet). Nothing runs while the Mac sleeps, Tidemark included, so sleep is a gap, never a dip to zero.
- Point at a chart to read any moment, or at a gap for its times.

## Performance

CPU, the **GPU** (where the Mac reports it) and memory over 1 hour, 24 hours, 7 or 14 days (Tidemark remembers the range you chose), with the week's tidemark and the range's peak. Click a chart to see what every app was doing at that moment.

Memory's line is everything in use; the line under its heading, and the readout at any moment, splits it as Activity Monitor does: memory apps are using, **wired** memory the system keeps for itself and never moves to disk, and **compressed** memory macOS has squeezed to make room. When compressed and swap grow while pressure goes red, the Mac is short of memory. The GPU's line is how busy the graphics processor is, which video, games, 3D and machine-learning work load; apps' shares of it are not shown, because macOS doesn't report them.

**Network** shows download and upload as two charts on the same scale, each with its busiest moment, and how much moved each way in the range. It counts every network interface except the Mac talking to itself.

**Energy** shows the power the whole Mac draws, in watts; the battery's charge over the range, its health (its capacity now against what it was designed to hold) and charge cycles; what the last sleep cost the battery; and the apps that used the most energy. Power and battery come from the battery's own controller, so a Mac without a battery shows only the apps. Each app's energy is shown as its share of what all your apps used: the kernel's per-process count leaves out the display and macOS itself, so its watts would look smaller than they are. Only your own apps can be measured. Tidemark notes the battery either side of each sleep, which is how it knows what a sleep cost.

Smaller tiles show swap, disk reads and writes and the Mac's thermal state.

The panel on the right is a **snapshot**: the apps that used the most at the chosen moment (the range's CPU peak until you click somewhere). Processes are grouped by the app they belong to, so Chrome's many helpers count as Google Chrome. CPU is a share of the whole Mac, not of one core. **Ask why this happened** puts the moment to Ask; **Activity Monitor** opens macOS's own, live.

**Incidents** are recorded when something crosses a line: CPU above 90% for 30 seconds, memory pressure going red (critical), or swap growing by 1 GB within 10 minutes. Each keeps a snapshot of what every app was doing at its peak. Click one to jump to it.

**Crashes and hangs** lists the apps that crashed or stopped responding in the last 14 days, how often, and when last, from the reports macOS writes (the ones Console shows under Crash Reports). Faults macOS only noted, which the app survived, are left out.

Tidemark samples the whole Mac every 2 seconds while its window is in front or the menu-bar window is open, and every 5 seconds otherwise, when it reads each app only every 15 seconds; behind other windows the screen itself refreshes at most once a minute. It keeps one row a minute: the minute's average and its peak, so a short spike is never averaged away.

## Disk

Every folder as a block inside its parent's block, sized by what it takes up, so the big things are big. The rest of this guide's disk sections (the map, Getting around, Files in a folder, Free space, Cloud folders, Scanning) are about this screen.

**Growth this week** outlines the folders that grew in the last week and tags each with how much. A folder is tagged where the growth actually landed: if most of ~/.colima's growth is in one folder inside it, that folder is tagged, not ~/.colima and everything above it. Hovering any folder shows how much it changed this week. Growth compares two scans at least 20 hours apart, so it shows from the day after the first scan; the switch hides it.

## The map

Every folder is drawn nested inside its parent, with its name and size where the block is big enough. Sizes are **allocated bytes on disk**, not file lengths, because the question is what is using the disk.

- The files sitting directly in a folder are one grey dashed block, labelled with their count.
- Folders too small to draw at the current zoom are folded into a **more folders** block, so a folder's area is always its total.
- **Colour is nesting level**, in rainbow order from the scan root: red, orange, amber, green, teal, blue, indigo, violet. The level is counted from the root, so a folder keeps its colour as you zoom.
- Hover a block for its full path, size on disk, item count and share of its parent.

## Getting around

- **Click a folder block to zoom in.** Clicking its grey loose-files block opens the same folder.
- The **breadcrumb** at the top names where you are; click an earlier name to go back up. **Esc** or **⌘↑** go up one level.
- The **‹ ›** buttons (⌘[ and ⌘]) step back and forward through where you have been, like Finder.
- **Choose a folder** in the sidebar (⌘O) scans a different folder; the five most recent roots sit below it. The default is the whole boot volume.

## Finder and the right-click menu

- **Reveal in Finder**: the arrow on a block's header, ⌘-click on the block, or the arrow beside the breadcrumb for the folder on screen (⌘⇧R).
- **Right-click** a block for a menu: Reveal in Finder, Zoom in, Copy path, Get Info.

## Files in a folder

When a folder holds files directly, a files box lists them, and it fills the map area when the folder contains only files. The table shows Name, Date Modified, Size, On Disk and Kind, like Finder.

- Type in the box at the top to filter by name. It covers the files directly in this folder, not subfolders.
- Click a column heading to sort; click again to reverse.
- Double-click a file, or press its arrow, to reveal it in Finder.
- A folder with thousands of files loads a page at a time as you scroll.
- **Size** is the file's length; **On Disk** is the space it takes. Listing files never reads their contents or downloads online-only cloud files.

## Free space

At the top of a whole-disk scan (the boot volume, or a disk under /Volumes) the disk's free space is drawn as a dotted green block beside the folders, so the map shows the whole disk. The **Free space** switch in the title bar hides it, and the app remembers the choice. It is not drawn inside a folder, or for a scan of a single folder.

The sidebar's used figure comes from the filesystem and is larger than the scan total: it includes local Time Machine snapshots, purgeable space, swap, the recovery volume and folders the scan skips or cannot read. Hover the bar for the breakdown.

## Cloud folders

OneDrive, iCloud Drive and anything else under Library/CloudStorage or Library/Mobile Documents carry a double edge and show two numbers: what is on disk and what is in the cloud. A file that is online-only has a length but takes no space, so it counts as nothing on disk and appears only in the folder's "in cloud" figure.

So that an online-only cloud folder does not vanish from the map, its block is never drawn smaller than 2% of its cloud size. Inside such a folder the blocks are proportional to cloud size, and the tooltip says so.

## Scanning

**Reload** (⌘R) re-scans the whole tree. While a scan runs the button reads **Stop**; stopping keeps the last completed scan on screen. There is no pause.

The scan is a parallel walk across every core; a full boot volume of about a million items takes on the order of half a minute on an SSD. It does not follow symlinks, counts a file with several hard links once, and does not descend into /System/Volumes, /Volumes, /dev or the virtual folders.

Every completed scan is **cached**. On launch the cached map is drawn at once, and a fresh scan runs behind it if the cached one is more than an hour old. The strip under the title bar shows what is being scanned, the items counted so far and a percentage measured against the previous scan; the title bar says when the scan on screen finished.

The disk is also scanned on the schedule Settings sets (daily by default), by the app or by the background helper; a scan the helper finishes replaces the map on screen.

## Full Disk Access

Folders the app is not allowed to read are skipped and counted. If any were skipped and the app does not have Full Disk Access, a banner says so and opens System Settings. Grant it under Privacy & Security › Full Disk Access, then Reload, and Mail, Safari, Photos and the other protected folders are included.

Some folders stay unreadable even then: system folders owned by root, such as /private/var/folders. Full Disk Access opens your own protected folders, not those. The sidebar shows how many were skipped.

## Trends

The disk over 14 days, 30 days, 3 months or 12 months (remembered, like Performance's range): how much it grew, how fast, and when it would be full at that rate. Up to 14 days the line is the used space hourly; beyond, each day's high. Its tidemark is the most it held in the range; the list beside it names the biggest one-day rises.

**Top folders that rose** lists where the growth landed over the range. Up to 14 days this compares scans; beyond that it compares the daily record of the 40 biggest folders, so it is coarser. **When it gets busy** shades each hour of each weekday by its average CPU over the history, darker for busier; point at a cell for its value.

At the foot, the range's highs for CPU, memory, swap and the disk, and questions for Ask.

## History

Everything Tidemark remembers is in one file, ~/Library/Application Support/Tidemark/history.sqlite, on this Mac only:

- Every minute's CPU, GPU, memory (and what it was made of), swap, memory pressure, disk, network, power and battery, with the busiest apps of that minute, every incident, every sleep and every crash or hang, for the days Settings keeps (14 at most).
- The disk's used space about every hour, and the folders of 100 MB or more from each scan, for the same days.
- One row a day of the disk's used space and its biggest folders, for a year whatever the setting, so the Trends screen can go back 12 months.

Tidemark records while it is running: closing the window keeps it running in the menu bar, and Open at Login starts it when you log in. With **Keep recording when quit** on (Settings), a background helper records even while the app is quit.

## Settings

A few things to choose (⌘,):

- **Keep recording when quit** registers a small background helper: Tidemark's own program, with no window, that records while the app is quit and starts when you log in. macOS lists it in System Settings › General › Login Items › Allow in the Background, and may ask you to allow it there. While both are running only one records; the other shows what it records.
- **Open at Login** starts Tidemark in the menu bar when you log in. It is the same setting as System Settings › General › Login Items. It works in the installed app, not in a development build.
- **Keep history for** 14, 7 or 3 days. Shortening it forgets the older days at once. The year of daily disk totals is kept either way.
- **Scan the disk**: Daily (the default) runs one full scan of the disk a day, when the Mac has been idle for 5 minutes and is on power; if it is never on power it still scans once the last scan is two days old. Hourly scans every hour, which costs some battery. Off scans only when you press Reload.
- **Appearance**: Match the Mac (the default) follows the Mac's light or dark appearance; Light and Dark keep Tidemark that way whatever the Mac does. The menu-bar window follows too.

At the foot, Settings shows how much space the history takes and whether Tidemark has Full Disk Access.

## Ask

**Ask** (⌘K) opens a chat about your Mac. It puts what Tidemark knows to an AI: the folder on screen and its biggest folders with sizes, what grew on the disk this week, and the performance record (now, today's highs, the week's tidemarks, incidents and the apps that used the most). Ask why the Mac was slow on Wednesday, or where space can be freed safely. Start with one of the suggested questions, or type your own; follow-ups carry on the same conversation.

- Ask runs through an AI command-line tool already installed and signed in on this Mac: Claude Code, Codex, Antigravity or GitHub Copilot. No account or key is set up in the app. If none is installed the Ask button is disabled and says so.
- The model menu in the panel's header lists every model those tools offer, grouped by tool. Claude Opus is the default; the choice is remembered.
- The AI sees sizes, names, paths, app names and the recorded figures only. It cannot read your files, and the app deletes and changes nothing: it advises, and you act in Finder, Terminal or the app concerned.
- **Copy** under an answer copies it to the clipboard as text and as formatted text, for pasting into a note or a message.
- **New chat** starts over about what is on screen now. Zooming to another folder and asking a follow-up sends the new folder with the question.

## Menu bar

Closing the window hides it and takes Tidemark out of the Dock; it keeps recording in the menu bar. The menu-bar icon gets a mark when something needs attention (memory running high, the CPU busy, swap growing, the battery low, the disk nearly full).

Clicking it opens a small window: how the Mac is doing in a sentence; CPU, memory, network, energy and disk with a line for the last 15 minutes (the disk's for the week) and today's high; what is using the most right now; and, when there is something to say, a line about the week (an app holding much of your memory, or the disk growing by a gigabyte or more). When something needs attention, the main action takes you to it (Show what's using memory, say). Below that are Open Tidemark (⌘O), Settings (⌘,) and Quit (⌘Q); ⌘2 and ⌘3 open Performance and Disk. The shortcuts work while it is open, and Esc closes it. Quitting stops the recording unless the background helper is on (Settings).

## Keyboard shortcuts

| Keys | What they do |
|---|---|
| ⌘1 ⌘2 ⌘3 ⌘4 | Now, Performance, Disk, Trends |
| ⌘, | Settings |
| ⌘[ and ⌘] | Back and forward through where you have been, across screens and folders |
| ⌘K | Ask about the disk or performance |
| ? or ⌘? | This help (Help › Tidemark Help in the menu bar) |
| ⌘R | Re-scan what the Disk screen shows: the disk, or the folder you chose |
| ⌘O | Choose a folder to scan |
| Esc or ⌘↑ | Up one level on the Disk screen (Esc first closes an open menu or panel) |
| ⌘⇧R | Reveal the folder on screen in Finder |
| ⌘-click a block | Reveal that folder in Finder |
