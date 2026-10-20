## Recent

Every live measure in detail, over 1 hour, 24 hours, 7 or 14 days.

- The range and the tab you pick are remembered, and a card's **Details** on Now or a row of the <!-- mac -->menu-bar<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->notification-area<!-- /windows --> window opens its tab.
- **All** shows one chart a measure, each the same size and lined up in time: CPU, GPU (where the computer reports it), memory, disk reads, disk writes, download, upload, energy and disk space.
- The other tabs give one measure more room, a chart for each of its parts, lined up the same way, with its busiest apps, incidents and busiest moments below. Charts of other measures that tend to move with it come last, under **For comparison**, which says why:
  - **CPU**: the CPU, its performance cores and its efficiency cores (where the processor has both kinds<!-- windows -->: Intel's 12th generation and later<!-- /windows -->); the apps using the most CPU, CPU incidents, <!-- mac -->the Mac's thermal state<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->whether Windows slowed the processor to cool it<!-- /windows --><!-- github --> (Windows)<!-- /github -->, and the apps that crashed or hung.
  - **GPU**: the GPU, then the CPU and the power drawn for comparison, and its busiest moments. <!-- mac -->macOS<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->Windows<!-- /windows --> doesn't report apps' shares of the GPU.<!-- windows --> Windows' figure is its GPU counters' busiest engine, as Task Manager's GPU figure is.<!-- /windows -->
  - **Memory**: memory in use, then what it's made of <!-- mac -->as Activity Monitor splits it (what apps use, **wired**, the system's own and never moved to disk, and **compressed**)<!-- /mac --><!-- github --> on a Mac, or on Windows <!-- /github --><!-- windows -->(apps and the system, **non-paged**, the kernel's own and never moved to disk, and **compressed**, Windows' Memory Compression store)<!-- /windows -->, swap<!-- windows --> (the page file in use)<!-- /windows --> and pressure; the apps holding the most and memory incidents. Compressed and swap growing while pressure turns red means the computer is short of memory.
  - **Disk I/O**: reads and writes, then swap for comparison (heavy swapping shows up as disk writes); the busiest moments and the apps using the disk.<!-- windows --> Windows' figure for each app counts all its reads and writes (files, network and devices), which Windows doesn't split, so an app's figure can be larger than the disk's.<!-- /windows -->
  - **Network**: download and upload, and the busiest of each. Every interface counts except the computer talking to itself.<!-- mac -->
  - **Energy**: the power the whole Mac draws and battery charge, then the CPU and GPU that most of it follows for comparison, the apps using the most energy and the battery's health, cycles and what the last sleep cost.<!-- /mac --><!-- windows -->
  - **Energy**: the power the PC draws on battery and the battery's charge, then the CPU and GPU that most of it follows for comparison, the battery's health, cycles and what the last sleep cost.<!-- /windows -->
  - **Disk space**: the disk's used space, recorded hourly, over the range (the last 24 hours on the 1-hour range), its free space, the change and the week's tidemark, and what grew this week.<!-- mac -->
- Power and battery come from the battery's controller, so a Mac without a battery shows only the apps.
- Each app's energy is a share of what the busiest apps used. macOS's per-app count leaves out the display and the system, so watts would look too small. Only your own apps can be measured.
- **Crashes and hangs**: apps that crashed or stopped responding within the history setting (14 days at most), how often and when last, from the reports macOS writes (Console's Crash Reports). Faults an app survived are left out.<!-- /mac --><!-- windows -->
- Power and battery come from the battery's driver, which gives the power drawn only while the PC runs on battery: on the adapter, and on a PC with no battery, there's no power figure. Charge, health (capacity against design), time left on battery and cycles, where the battery reports them (many don't report cycles).
- Windows doesn't measure apps' energy, so there are no energy lists by app.
- **Memory pressure**: Windows has none, so Tidemark works it out on the same three levels from how much of the commit limit (memory plus the page file) is committed, how much memory is free, and Windows' own low-memory signal. Red (critical) means Windows is about to start trimming programs or running out of commit.
- **Crashes and hangs**: apps that crashed, or stopped responding and were closed, within the history setting (14 days at most), how often and when last, from Windows' Application event log (what Reliability Monitor shows).<!-- /windows -->

The card on the right:

- **Snapshot**: the moment picked on any chart, or a peak dot, or a busiest-moments row. Until you pick one it is at the range's peak of the tab's own measure (the CPU's on All).
- It leads with what the line shows (that point's average) beside the peak inside it, and lists the apps: by their average over the point's minutes when you click the line, at their busiest when you pick a peak, or as an incident's own snapshot. The list says which. A thin line marks the moment on every chart of the tab.
- Apps are sorted by the chart's measure: CPU, memory, energy or disk. <!-- mac -->macOS<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->Windows<!-- /windows --> doesn't split the GPU or the network<!-- windows --> (or energy)<!-- /windows --> by app, so those list the busiest apps by CPU, and say so. Helpers count as their app, so Chrome's are Google Chrome. CPU is a share of the whole computer.
- On the **Disk space** tab the card shows what's using the space instead: the scanned folder's largest folders (click one to open it in Disk Map), the space the scan can't see, and the last scan. Click the disk chart there for that hour's used and free space.
- **Ask why this happened** puts the moment to Ask; **<!-- mac -->Activity Monitor<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->Task Manager<!-- /windows --><!-- github --> (Windows)<!-- /github -->** opens <!-- mac -->macOS's<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->Windows'<!-- /windows --> own.
- **Incidents** are recorded when CPU stays above 90% for 10 seconds, memory pressure turns red, or swap grows by 1 GB in 10 minutes. Each keeps a snapshot of the 12 busiest apps at its peak; click one to see it.
- While the record is shorter than the history setting, the sidebar's **Record** says how far back it goes.

How often Tidemark reads:

- Every 2 seconds while its window is in front or the <!-- mac -->menu-bar<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->notification-area<!-- /windows --> window is open.
- Otherwise every 5 seconds, reading each app every 15, so an app's peak is its busiest 2 or 15 seconds.
- Behind other windows, the screen refreshes at most once a minute.
- It keeps one row a minute, with the average and the peak (each app's too), so a short spike is never averaged away.
