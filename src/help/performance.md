## Recent

Every live measure in detail, over 1 hour, 24 hours, 7 or 14 days.

- The range and the tab you pick are remembered, and a card's **Details** on Now or a row of the menu opens its tab.
- **All** shows one chart a measure, each the same size and lined up in time: CPU, GPU (where the Mac reports it), memory, disk reads, disk writes, download, upload, energy and disk space.
- The other tabs give one measure more room, a chart for each of its parts, lined up the same way, with its busiest apps, incidents and busiest moments below:
  - **CPU**: the CPU, its performance cores and its efficiency cores; the apps using the most CPU, CPU incidents, the Mac's thermal state, and the apps that crashed or hung.
  - **GPU**: the GPU beside the CPU and the power drawn, and its busiest moments. macOS doesn't report apps' shares of the GPU.
  - **Memory**: memory in use, then what it's made of as Activity Monitor splits it (what apps use, **wired**, the system's own and never moved to disk, and **compressed**), swap and pressure; the apps holding the most and memory incidents. Compressed and swap growing while pressure turns red means the Mac is short of memory.
  - **Disk I/O**: reads, writes and swap; the busiest moments and the apps using the disk.
  - **Network**: download and upload, and the busiest of each. Every interface counts except the Mac talking to itself.
  - **Energy**: the power the whole Mac draws, battery charge, the CPU and GPU that most of it follows, the apps using the most energy and the battery's health, cycles and what the last sleep cost.
  - **Disk space**: the disk's used space, recorded hourly, over the range (the last 24 hours on the 1-hour range), its free space, the change and the week's tidemark, and what grew this week.
- Power and battery come from the battery's controller, so a Mac without a battery shows only the apps.
- Each app's energy is a share of what the busiest apps used. macOS's per-app count leaves out the display and the system, so watts would look too small. Only your own apps can be measured.
- **Crashes and hangs**: apps that crashed or stopped responding in the last 14 days, how often and when last, from the reports macOS writes (Console's Crash Reports). Faults an app survived are left out.

The card on the right:

- **Snapshot**: the moment picked on any chart, or a peak dot, or a busiest-moments row. Until you pick one it is at the range's peak of the tab's own measure (the CPU's on All).
- It leads with what the line shows (that point's average) beside the peak inside it, and lists the apps: by their average over the point's minutes when you click the line, at their busiest when you pick a peak, or as an incident's own snapshot. The list says which. A thin line marks the moment on every chart of the tab.
- Apps are sorted by the chart's measure: CPU, memory, energy or disk. macOS doesn't split the GPU or the network by app, so those list the busiest apps by CPU, and say so. Helpers count as their app, so Chrome's are Google Chrome. CPU is a share of the whole Mac.
- On the **Disk space** tab the card shows what's using the space instead: the scanned folder's largest folders (click one to open it in Disk Map), the space the scan can't see, and the last scan. Click the disk chart there for that hour's used and free space.
- **Ask why this happened** puts the moment to Ask; **Activity Monitor** opens macOS's own.
- **Incidents** are recorded when CPU stays above 90% for 30 seconds, memory pressure turns red, or swap grows by 1 GB in 10 minutes. Each keeps a snapshot of every app at its peak; click one to see it.
- While the record is shorter than the history setting, the sidebar's **Record** says how far back it goes.

How often Tidemark reads:

- Every 2 seconds while its window is in front or the menu-bar window is open.
- Otherwise every 5 seconds, reading each app every 15, so an app's peak is its busiest 2 or 15 seconds.
- Behind other windows, the screen refreshes at most once a minute.
- It keeps one row a minute, with the average and the peak (each app's too), so a short spike is never averaged away.
