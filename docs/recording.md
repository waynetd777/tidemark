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
| Memory split (apps, wired, compressed) | The kernel's VM statistics |
| Memory pressure | `kern.memorystatus_vm_pressure_level` |
| GPU | The GPU driver's own statistics (`ioreg`) |
| Power, battery, health | The battery's controller (`ioreg`), every 10 seconds |
| Per-app energy | The kernel's energy count per process, shown as shares |
| Crashes and hangs | macOS's reports in DiagnosticReports, every 5 minutes; survived faults left out |

## The history

One SQLite file: `~/Library/Application Support/Tidemark/history.sqlite`, on this Mac only.

| What | Kept for |
|---|---|
| Every minute's readings and busiest apps | 14 days at most (Settings) |
| Incidents, sleeps, crashes | The same |
| Disk space hourly, and big folders from each scan | The same |
| One row a day of disk space and its top folders | A year |

## Sleep and gaps

- Nothing runs while the Mac sleeps, so sleep is a gap, never a dip to zero.
- Tidemark notes the battery either side of a sleep, so it knows what the sleep cost.
- Charts shade gaps **Asleep** or **Not recording** (Tidemark quit, paused or not yet started).

## The background helper

- **Keep recording when quit** (Settings) registers the app's own binary as `Tidemark --record`, a launch agent registered with `SMAppService` (the plist is in `Contents/Library/LaunchAgents`).
- The app and the helper share a lock: whichever holds it records, and the other follows its live readings in `live.json`, taking over if it stops.
- The helper also runs the scheduled disk scans.
