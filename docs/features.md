# Features

Tidemark's screens, briefly. The app's own help (press `?`) has the detail.

## Now (⌘1)

How your Mac is doing, at a glance.

<picture><source media="(prefers-color-scheme: dark)" srcset="images/now-dark.png"><img alt="The Now screen" src="images/now-light.png" width="900"></picture>

- A sentence on how the Mac is doing.
- Eight cards: CPU, GPU, memory, disk I/O, network download, network upload and energy over the last hour; disk space over the last 24 hours.
- The week's tidemarks (peaks, incidents, crashes) and what grew on the disk.
- **Details** on a card opens the screen with more.

## Performance (⌘2)

Every live measure in detail, over 1 hour to 14 days.

<picture><source media="(prefers-color-scheme: dark)" srcset="images/performance-dark.png"><img alt="The Performance screen" src="images/performance-light.png" width="900"></picture>

- CPU, GPU and memory charts, with each point's peak as a band and the week's tidemark.
- Memory split into apps, wired and compressed.
- Disk I/O and network, each way; energy with battery health and what sleep cost; disk space, hourly.
- Click any moment to see what every app was doing.
- **Incidents**: CPU above 90% for 30 seconds, memory pressure red, or swap up 1 GB in 10 minutes, each with a snapshot.
- Apps that crashed or hung, from macOS's own reports.

## Disk Map (⌘3)

What is using the disk: every folder as a block, sized by what it takes up. See [Disk Map](disk-map.md).

<picture><source media="(prefers-color-scheme: dark)" srcset="images/disk-map-dark.png"><img alt="The Disk Map screen" src="images/disk-map-light.png" width="900"></picture>

## Trends (⌘4)

The disk over weeks and months, and when the Mac is busiest.

<picture><source media="(prefers-color-scheme: dark)" srcset="images/trends-dark.png"><img alt="The Trends screen" src="images/trends-light.png" width="900"></picture>

- Disk space over 7 days to 12 months: how fast it grows, and when it would be full.
- The folders that rose, CPU by weekday and hour, and the range's highs.

## Settings (⌘,)

<picture><source media="(prefers-color-scheme: dark)" srcset="images/settings-dark.png"><img alt="Settings" src="images/settings-light.png" width="900"></picture>

| Setting | What it does |
|---|---|
| **Keep recording when quit** | A background helper records while the app is quit. |
| **Open at Login** | Starts Tidemark in the menu bar at login. |
| **Keep history for** | 14, 7 or 3 days of detail. |
| **Scan the disk** | Daily (idle and on power), hourly or off. |
| **Appearance** | Match the Mac, light or dark. |

## Menu bar

Closing the window keeps Tidemark recording in the menu bar.

<picture><source media="(prefers-color-scheme: dark)" srcset="images/menu-bar-dark.png"><img alt="The menu-bar window" src="images/menu-bar-light.png" width="380"></picture>

- The icon is marked when something needs attention.
- Click it for the live readings, what is using the most, and the action that helps.

## Ask (⌘K)

A chat with an AI about your Mac, through a command-line tool already signed in on it (Claude Code, Codex, Antigravity or GitHub Copilot).

- Ask why the Mac was slow, or where space can be freed safely.
- It sees names, sizes and figures only, and Tidemark changes nothing.

## Charts

- Lines are averages and run up to now; a band behind shows each point's peak.
- A dashed line is the tidemark: the high-water mark.
- Sleep is shaded **Asleep**; time Tidemark wasn't running is hatched **Not recording**.
- The measure that needs attention turns amber (warning) or red (critical).
