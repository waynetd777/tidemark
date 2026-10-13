// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

/// Decimal units, as macOS reports them. Three significant figures at most. For disk sizes.
export function fmt(b: number): string {
  return sized(b, 1e3);
}

/// Binary units (a GB is 2^30 bytes), as Activity Monitor shows memory. For memory and swap only.
export function fmtMem(b: number): string {
  return sized(b, 1024);
}

/// Bytes in units of `k`: TB to two places trimmed, GB to one place below 100, the rest whole.
/// Rounded before the unit is settled, so 99.96 GB reads "100 GB" and 999.6 GB "1 TB".
function sized(b: number, k: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (i < 4 && b >= k ** (i + 1)) i++;
  for (;;) {
    const x = b / k ** i;
    const s =
      i === 4
        ? x.toFixed(2).replace(/\.?0+$/, "")
        : i === 3 && x < 99.95
          ? x.toFixed(1)
          : String(Math.round(x));
    if (i < 4 && Number(s) >= k) {
      i++;
      continue;
    }
    return `${s} ${units[i]}`;
  }
}

export function fmtN(n: number): string {
  return n.toLocaleString("en-GB");
}

/// "/Users/alex/Projects" -> "~/Projects" when the path is under the home folder.
export function abbrev(path: string, home: string): string {
  if (path === home) return "~";
  if (home && path.startsWith(home + "/")) return "~" + path.slice(home.length);
  return path;
}

export function fmtDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = Math.round(ms / 1000);
  if (s < 90) return `${s} s`;
  const m = Math.floor(s / 60);
  return `${m} min ${s - m * 60} s`;
}

/// "today 09:12", "yesterday 18:40", or "Mon 3 Sep 09:12".
export function fmtWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const hm = d.toTimeString().slice(0, 5);
  const now = new Date();
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86400000);
  if (diff === 0) return `today ${hm}`;
  if (diff === 1) return `yesterday ${hm}`;
  return `${d.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" })} ${hm}`;
}

export function parentPath(path: string): string | null {
  if (path === "/" || !path.includes("/")) return null;
  const i = path.lastIndexOf("/");
  return i === 0 ? "/" : path.slice(0, i);
}

/// A percentage, whole.
export function pct(n: number): string {
  return `${Math.round(n)}%`;
}

/// Bytes a second, compactly. Thresholds sit where the smaller unit would round up to 1000.
export function fmtRate(b: number): string {
  if (b >= 999.5e6) return `${(b / 1e9).toFixed(1)} GB/s`;
  if (b >= 99.95e6) return `${Math.round(b / 1e6)} MB/s`;
  if (b >= 999.5e3) return `${(b / 1e6).toFixed(1)} MB/s`;
  if (b >= 999.5) return `${Math.round(b / 1e3)} KB/s`;
  return `${Math.round(b)} B/s`;
}

/// A unix time (seconds) as "11:20" today, "yesterday 11:20", or "Tue 14:12" this week, else
/// "3 Oct".
export function fmtAt(ts: number, withTime = true): string {
  const d = new Date(ts * 1000);
  const hm = d.toTimeString().slice(0, 5);
  const now = new Date();
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86400000);
  if (diff === 0) return withTime ? hm : "today";
  if (diff === 1) return withTime ? `yesterday ${hm}` : "yesterday";
  if (diff < 7) {
    const wd = d.toLocaleDateString("en-GB", { weekday: "short" });
    return withTime ? `${wd} ${hm}` : wd;
  }
  return d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

/// A unix time (seconds) as an event in a list: "Today at 16:29", "Yesterday at 19:30",
/// "Wed 7 Oct at 09:12", with the year when it isn't this one.
export function fmtEvent(ts: number): string {
  const d = new Date(ts * 1000);
  const hm = d.toTimeString().slice(0, 5);
  const now = new Date();
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86400000);
  if (diff === 0) return `Today at ${hm}`;
  if (diff === 1) return `Yesterday at ${hm}`;
  const date = d.toLocaleDateString("en-GB", {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: d.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
  return `${date.replace(",", "")} at ${hm}`;
}

/// A signed size change: "+6.1 GB", "−0.3 GB".
export function fmtDelta(b: number): string {
  return `${b < 0 ? "\u2212" : "+"}${fmt(Math.abs(b))}`;
}

/// Seconds as "6 days, 3 hours" or "3 hours, 12 min".
export function fmtUptime(s: number): string {
  const d = Math.floor(s / 86400),
    h = Math.floor((s % 86400) / 3600),
    m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d} day${d === 1 ? "" : "s"}, ${h} hour${h === 1 ? "" : "s"}`;
  if (h > 0) return `${h} hour${h === 1 ? "" : "s"}, ${m} min`;
  return `${m} min`;
}

/// Watts: "10.5 W", "34 W".
export function fmtW(w: number): string {
  return `${w < 10 ? w.toFixed(1) : Math.round(w)} W`;
}

/// Minutes as "3 h 15 min" or "42 min".
export function fmtMins(m: number): string {
  const h = Math.floor(m / 60);
  return h ? `${h} h ${m % 60} min` : `${m} min`;
}

/// The battery in a few words: "54% · 3 h 15 min left", "Charging · 54%", "On power · 100%".
export function batteryLine(b: {
  percent: number;
  charging: boolean;
  on_ac: boolean;
  minutes_left: number | null;
}): string {
  const p = `${Math.round(b.percent)}%`;
  if (b.charging)
    return `Charging · ${p}${b.minutes_left ? ` · full in ${fmtMins(b.minutes_left)}` : ""}`;
  if (b.on_ac) return `On power · ${p}`;
  return `${p}${b.minutes_left ? ` · ${fmtMins(b.minutes_left)} left` : ""}`;
}

/// A count, compactly: "999", "12K", "2.6M", "1.2B".
export function fmtCount(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e4) return `${Math.round(n / 1e3)}K`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
  return `${Math.round(n)}`;
}

/// What one point of a bucketed series stands for: "minute", "5-minute", "42-minute".
export function bucketWord(secs: number): string {
  const m = Math.max(1, Math.round(secs / 60));
  return m === 1 ? "minute" : `${m}-minute`;
}
