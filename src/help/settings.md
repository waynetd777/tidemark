## Settings

A few choices; everything else looks after itself.

- **Keep recording when quit**: a small background helper (Tidemark's own program, no window) records while the app is quit and starts at login. macOS lists it as "Tidemark" under System Settings › General › Login Items › Allow in the Background; switched off there, Settings says so, and switching it back on there is the way to allow it again. When both run, one records and the other shows what it records.
- **Open at Login**: starts Tidemark in the menu bar at login. It's the same switch as System Settings › General › Login Items, and works in the installed app only.
- **Keep every minute for**: 14, 7 or 3 days, the detail Recent shows. Shortening it forgets older minutes at once; the year of daily rows (for Trends) and of daily disk totals is kept either way.
- **Scan the disk**: **Daily** (the default) scans once a day when the Mac has been idle for 5 minutes and is on power, or after two days if it never is. **Hourly** scans every hour and costs some battery. **Off** scans only when you press Reload.
- **Appearance**: **Match the Mac** (the default), **Light** or **Dark**. The menu-bar window follows.
- **About** shows the version; **Website** opens tidemark.davies.co.za. Tidemark › About Tidemark shows it too, as a link, and Help › Tidemark Website opens it.
- The foot shows how much space the history takes, and whether Tidemark has Full Disk Access.

## History

Everything Tidemark remembers is in one file on this Mac: ~/Library/Application Support/Tidemark/history.sqlite.

| What | Kept for |
|---|---|
| Every minute's CPU, GPU, memory and its split, swap, pressure, disk, network, power and battery, with the busiest apps | The history setting (14 days at most) |
| Incidents, sleeps, crashes and hangs | The same |
| The disk's used space hourly, and folders of 100 MB or more from each scan | The same |
| One row a day of every measure (averages, peaks, totals, incidents and crashes, the busiest apps), for Trends | A year, whatever the setting |
| One row a day of disk space and its biggest folders | A year, whatever the setting |

- Tidemark records while it runs. Closing the window keeps it running in the menu bar.
- **Keep recording when quit** (Settings) records even while the app is quit.
