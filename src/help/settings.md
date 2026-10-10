## Settings

A few choices; everything else looks after itself.

- **Keep recording when quit**: a small background helper (Tidemark's own program, no window) records while the app is quit and starts at login. macOS lists it under System Settings › General › Login Items › Allow in the Background, and may ask you to allow it. When both run, one records and the other shows what it records.
- **Open at Login**: starts Tidemark in the menu bar at login. It's the same switch as System Settings › General › Login Items, and works in the installed app only.
- **Keep history for**: 14, 7 or 3 days. Shortening it forgets older days at once; the year of daily disk totals is kept either way.
- **Scan the disk**: **Daily** (the default) scans once a day when the Mac has been idle for 5 minutes and is on power, or after two days if it never is. **Hourly** scans every hour and costs some battery. **Off** scans only when you press Reload.
- **Appearance**: **Match the Mac** (the default), **Light** or **Dark**. The menu-bar window follows.
- The foot shows how much space the history takes, and whether Tidemark has Full Disk Access.

## History

Everything Tidemark remembers is in one file on this Mac: ~/Library/Application Support/Tidemark/history.sqlite.

| What | Kept for |
|---|---|
| Every minute's CPU, GPU, memory and its split, swap, pressure, disk, network, power and battery, with the busiest apps | The history setting (14 days at most) |
| Incidents, sleeps, crashes and hangs | The same |
| The disk's used space hourly, and folders of 100 MB or more from each scan | The same |
| One row a day of disk space and its biggest folders | A year, whatever the setting |

- Tidemark records while it runs. Closing the window keeps it running in the menu bar.
- **Keep recording when quit** (Settings) records even while the app is quit.
