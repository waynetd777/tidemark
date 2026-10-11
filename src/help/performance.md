## Recent

Every live measure in detail, over 1 hour, 24 hours, 7 or 14 days.

- The range you pick is remembered. Click a chart to see what every app was doing at that moment.
- **All** shows one chart a measure, each the same size. The other tabs (**CPU**, **GPU**, **Memory**, **Disk I/O**, **Network**, **Energy**) give one measure more room, with what goes with it: its parts, the apps that used the most, its incidents and its busiest moments. The tab you pick is remembered, and a card's **Details** opens its tab.
- **CPU**, **GPU** (where the Mac reports it) and **Memory** charts, each with the week's tidemark and the range's peak.
- Memory splits as Activity Monitor does: what apps use, **wired** (the system's own, never moved to disk) and **compressed**. Compressed and swap growing while pressure turns red means the Mac is short of memory.
- The GPU line is how busy the graphics processor is (video, games, 3D, machine learning). macOS doesn't report apps' shares of it.
- **Disk I/O**: reads and writes as two charts on one scale, with their busiest moments and totals for the range.
- **Network**: download and upload, the same way. Every interface counts except the Mac talking to itself.
- **Energy**: the power the whole Mac draws, battery charge, health (capacity now against its design) and cycles, what the last sleep cost, and the apps that used the most.
- Power and battery come from the battery's controller, so a Mac without a battery shows only the apps.
- Each app's energy is a share of what your apps used. macOS's per-app count leaves out the display and the system, so watts would look too small. Only your own apps can be measured.
- **Disk space**: the disk's used space, recorded hourly, over the range (the last 24 hours on the 1-hour range), with the week's tidemark.
- **Crashes and hangs** (on the CPU tab): apps that crashed or stopped responding in the last 14 days, how often and when last, from the reports macOS writes (Console's Crash Reports). Faults an app survived are left out.
- **Swap** is on the Memory tab, and the Mac's **thermal** state on the CPU, GPU and Energy tabs.

The panel on the right:

- **Snapshot**: the apps that used the most at the chosen moment (the range's CPU peak until you click). Helpers count as their app, so Chrome's are Google Chrome. CPU is a share of the whole Mac.
- **Ask why this happened** puts the moment to Ask; **Activity Monitor** opens macOS's own.
- **Incidents** are recorded when CPU stays above 90% for 30 seconds, memory pressure turns red, or swap grows by 1 GB in 10 minutes. Each keeps a snapshot of every app at its peak; click one to jump to it.

How often Tidemark reads:

- Every 2 seconds while its window is in front or the menu-bar window is open.
- Otherwise every 5 seconds, reading each app every 15.
- Behind other windows, the screen refreshes at most once a minute.
- It keeps one row a minute, with the average and the peak, so a short spike is never averaged away.
