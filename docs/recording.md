# Recording

How Tidemark reads your Mac, and what it keeps.

## Sampling

| When | How often |
|---|---|
| The window is in front, or the menu-bar window is open | Every 2 seconds |
| Otherwise | Every 5 seconds; each app every 15 |
| The window is behind others | The screen refreshes at most once a minute |

- Each minute keeps the average and the peak, and the busiest apps, so a short spike is never averaged away.
- Apps are grouped with their helpers (Chrome's helpers count as Google Chrome). CPU is a share of the whole Mac.

## Sources

| Measure | From |
|---|---|
| CPU, memory, swap, network, per-app use | The kernel, via `sysinfo` |
| Disk I/O | Every process's reads and writes, summed, from the per-app pass (every 15 seconds when no window is in front) |
| Disk space | `statfs` on the boot volume, hourly |
| Memory split (apps, wired, compressed) | The kernel's VM statistics |
| Memory pressure | `kern.memorystatus_vm_pressure_level` |
| GPU | The GPU driver's own statistics (`ioreg`), with the apps (every 15 seconds when no window is in front) |
| Power, battery, health | The battery's controller (`ioreg`), every 10 seconds; a Mac without a battery has none of these |
| Per-app energy | The kernel's energy count per process, shown as shares |
| Crashes and hangs | macOS's reports in DiagnosticReports, every 5 minutes; survived faults left out |

## The history

One SQLite file: `~/Library/Application Support/Tidemark/history.sqlite`, on this Mac only. Beside it in the same folder: the saved scans (`scans/`), `settings.json` and `live.json`.

| What | Kept for |
|---|---|
| Every minute's readings and busiest apps | 14 days at most (Settings) |
| Incidents, sleeps, crashes | The same |
| Disk space hourly, and big folders from each scan | The same |
| One row a day of every measure, for Trends (`perf_daily`: averages, peaks, totals, incident and crash counts, the busiest apps), rolled up from the minutes every ten minutes and before they are pruned | A year |
| One row a day of disk space and its top folders | A year |

## Sleep and gaps

- Nothing runs while the Mac sleeps, so sleep is a gap, never a dip to zero.
- Tidemark notes the battery either side of a sleep, so it knows what the sleep cost.
- Charts shade gaps **Asleep** or **Not recording** (Tidemark quit or not yet started).

## The background helper

- **Keep recording when quit** (Settings) runs the app's own binary as `Tidemark --record` from a plain launch agent: the app writes `~/Library/LaunchAgents/com.wayned.tidemark.helper.plist`, pointing at the installed app, and loads it with `launchctl`. It is listed in System Settings › General › Login Items › Allow in the Background, where it can be switched off.
- launchd restarts it only after a failure, at most every 5 minutes. Each launch of the installed app points the agent at its binary again if the app has moved, and restarts the helper after an update.
- It used to be registered with `SMAppService`, but macOS would not start a helper registered that way for an app with a self-signed certificate. The plain agent has a label of its own, because launchd refuses to start one that shares the old registration's label (macOS keeps a record under it).
- The app and the helper share a lock: whichever holds it records, and the other follows its live readings in `live.json`, taking over if it stops.
- Whichever process is recording also does the upkeep: the hourly disk space, crash reports, pruning and the scheduled disk scans. The other picks up the scan it saves.
