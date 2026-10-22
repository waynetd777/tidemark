// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Trends: every measure over a month to a year, a day at a time, laid out like Recent: tabs, one
// chart a measure, the day picked on them (its figures and busiest apps), the range's records,
// when each measure is usually busy, and the disk's fill projection and the folders that rose.
// Days come from perf_daily (a row a day, kept for a year); the by-hour shading from the minutes
// kept (14 days at most).

import { useAssistant } from "./assistant";
import { type ReactNode, useEffect, useState } from "react";
import { AllIcon, Labelled, MetricIcon } from "./icons";
import { metricOf } from "./metrics";
import { api } from "./api";
import { Alongside, Area, Heatmap, Legend, Snap, Spark, type Pt } from "./charts";
import { now, usePoll, usePolled } from "./perf";
import { isTab, TABS, tabFor, type Tab } from "./perfTabs";
import { useRemembered } from "./remember";
import type { AppUse, DiskPoint, Growth, PerfDay, Screen } from "./types";
import { isMac, MACHINE, OS } from "./keys";

const WIN_ENERGY =
  "Windows doesn't say which apps use energy: these were the day's busiest by CPU.";
import { abbrev, fmt, fmtDelta, fmtMem, fmtRate, fmtW, pct } from "./util";

const RANGES = [
  { label: "30 days", days: 30 },
  { label: "3 months", days: 91 },
  { label: "12 months", days: 365 },
];

/// What each tab shows here, where it differs from Recent's.
const TIPS: Record<Tab, string> = {
  all: "Every measure a day at a time, one chart each",
  cpu: "The CPU each day, its kinds of core, its records and when it's usually busy",
  gpu: "The GPU each day, beside the CPU and the power drawn",
  memory: "Memory each day, the swap and pressure highs, and when it's usually full",
  diskio: "What was read and written each day, and the busiest days",
  network: "What was downloaded and uploaded each day, and the busiest days",
  energy: "The energy used each day, the power drawn, and what draws it",
  disk: "The disk over the range, when it would fill, and the folders that rose",
};

const PRESSURE = (v: number) => (v >= 4 ? "Critical" : v >= 2 ? "Raised" : "Normal");
const wh = (v: number) => (v >= 1000 ? `${(v / 1000).toFixed(1)} kWh` : `${Math.round(v)} Wh`);

function dateLabel(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

function dayLabel(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString("en-GB", {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}

/// Day-on-day rises of 2 GB or more, biggest first, with the day they happened.
function jumps(points: DiskPoint[]): { ts: number; delta: number }[] {
  const byDay = new Map<string, DiskPoint>();
  for (const p of points) byDay.set(new Date(p.ts * 1000).toDateString(), p);
  const days = [...byDay.values()];
  const midnight = (ts: number) => new Date(ts * 1000).setHours(0, 0, 0, 0);
  const out: { ts: number; delta: number }[] = [];
  for (let i = 1; i < days.length; i++) {
    // Neighbouring days only: across a gap in the record the rise took more than a day.
    if (Math.round((midnight(days[i].ts) - midnight(days[i - 1].ts)) / 86400000) !== 1) continue;
    const d = days[i].used - days[i - 1].used;
    if (d >= 2e9) out.push({ ts: days[i].ts, delta: d });
  }
  return out.sort((a, b) => b.delta - a.delta).slice(0, 3);
}

/// One folder's size over the scans in the history, as a small line.
function FolderSpark({ path, days }: { path: string; days: number }) {
  const h = usePoll(() => api.folderHistory(path, days), 300_000, [path, days], {
    clear: true,
  });
  const pts: Pt[] = (h ?? []).map(([t, v]) => ({ t, v }));
  if (pts.length < 2) return <span className="spark-empty" />;
  const lo = Math.min(...pts.map((p) => p.v));
  return (
    <Spark
      pts={pts}
      min={lo * 0.95}
      max={Math.max(...pts.map((p) => p.v))}
      width={90}
      height={24}
      label={`${path} over time`}
    />
  );
}

/// A measure as Trends draws it: a day's figure (the line), its peak where there is one (the
/// band), and which of the day's busiest apps go with it. GPU and network aren't split by app,
/// so their days list the busiest by CPU.
interface Measure {
  name: string;
  what: string;
  avg: (d: PerfDay) => number | null;
  peak?: (d: PerfDay) => number | null;
  fmt: (v: number) => string;
  by: "cpu" | "mem" | "energy" | "disk";
  /// What a day's figure is: "average" unless it's a total ("read that day") or a high.
  avgLabel: string;
  note?: string;
  /// Shading by hour of the week, as `api.byHour` names it.
  hour?: string;
}

const M: Record<string, Measure> = {
  CPU: {
    name: "CPU",
    what: "CPU",
    avg: (d) => d.cpu,
    peak: (d) => d.cpu_max,
    fmt: pct,
    by: "cpu",
    avgLabel: "the day's average",
    hour: "cpu",
  },
  "Performance cores": {
    name: "Performance cores",
    what: "performance cores",
    avg: (d) => d.cpu_p,
    fmt: pct,
    by: "cpu",
    avgLabel: "the day's average",
  },
  "Efficiency cores": {
    name: "Efficiency cores",
    what: "efficiency cores",
    avg: (d) => d.cpu_e,
    fmt: pct,
    by: "cpu",
    avgLabel: "the day's average",
  },
  GPU: {
    name: "GPU",
    what: "GPU",
    avg: (d) => d.gpu,
    peak: (d) => d.gpu_max,
    fmt: pct,
    by: "cpu",
    avgLabel: "the day's average",
    note: `${OS} doesn't say which apps use the GPU: these were the day's busiest by CPU.`,
    hour: "gpu",
  },
  Memory: {
    name: "Memory",
    what: "memory in use",
    avg: (d) => d.mem,
    peak: (d) => d.mem_max,
    fmt: fmtMem,
    by: "mem",
    avgLabel: "the day's average",
    hour: "mem",
  },
  Swap: {
    name: "Swap",
    what: "swap",
    avg: (d) => d.swap_max,
    fmt: fmtMem,
    by: "mem",
    avgLabel: "the day's high",
  },
  Pressure: {
    name: "Pressure",
    what: "memory pressure",
    avg: (d) => d.pressure,
    fmt: PRESSURE,
    by: "mem",
    avgLabel: "the day's highest",
  },
  Read: {
    name: "Read",
    what: "read from the disk",
    avg: (d) => d.read,
    fmt,
    by: "disk",
    avgLabel: "read that day",
    note: "The apps are listed by what they read and wrote together.",
    hour: "disk",
  },
  Written: {
    name: "Written",
    what: "written to the disk",
    avg: (d) => d.written,
    fmt,
    by: "disk",
    avgLabel: "written that day",
    note: "The apps are listed by what they read and wrote together.",
    hour: "disk",
  },
  Downloaded: {
    name: "Downloaded",
    what: "downloaded",
    avg: (d) => d.down,
    fmt,
    by: "cpu",
    avgLabel: "downloaded that day",
    note: `${OS} doesn't say which apps use the network: these were the day's busiest by CPU.`,
    hour: "down",
  },
  Uploaded: {
    name: "Uploaded",
    what: "uploaded",
    avg: (d) => d.up,
    fmt,
    by: "cpu",
    avgLabel: "uploaded that day",
    note: `${OS} doesn't say which apps use the network: these were the day's busiest by CPU.`,
    hour: "up",
  },
  Energy: {
    name: "Energy",
    what: "energy used",
    avg: (d) => d.energy,
    fmt: wh,
    // Windows measures no app's energy, and the PC's power only on battery.
    by: isMac ? "energy" : "cpu",
    note: isMac ? undefined : WIN_ENERGY,
    avgLabel: isMac ? "used that day" : "used on battery that day",
    hour: "power",
  },
  Power: {
    name: "Power",
    what: "power drawn",
    avg: (d) => d.power,
    peak: (d) => d.power_max,
    fmt: fmtW,
    by: isMac ? "energy" : "cpu",
    note: isMac ? undefined : WIN_ENERGY,
    avgLabel: isMac ? "the day's average" : "the day's average on battery",
    hour: "power",
  },
};

/// Each tab's charts (its own measure first) and the measure its day and records are about.
const TAB_CHARTS: Record<Exclude<Tab, "disk">, string[]> = {
  all: ["CPU", "GPU", "Memory", "Read", "Written", "Downloaded", "Uploaded", "Energy"],
  cpu: ["CPU", "Performance cores", "Efficiency cores"],
  gpu: ["GPU"],
  memory: ["Memory", "Swap", "Pressure"],
  diskio: ["Read", "Written"],
  network: ["Downloaded", "Uploaded"],
  energy: ["Energy", "Power"],
};

/// Other measures a tab shows for comparison, and why.
const TAB_ALSO: Partial<Record<Tab, { why: string; charts: string[] }>> = {
  gpu: {
    why: "The CPU sets the GPU's work, and both draw power: a busy GPU day is usually busy in one or both.",
    charts: ["CPU", "Power"],
  },
  energy: {
    why: "The CPU and the GPU draw most of the power: a day's energy follows them.",
    charts: ["CPU", "GPU"],
  },
};

export default function Trends({
  home,
  bar,
  go,
  ask,
  focus,
  historyDays,
}: {
  home: string;
  bar: (left: ReactNode) => ReactNode;
  go: (s: Screen, target?: string) => void;
  ask: (q: string) => void;
  /// A link's target (a measure, as Recent's Details): its tab opens.
  focus?: string;
  /// Days of minutes kept, which the by-hour shading is drawn from.
  historyDays: number;
}) {
  const assistant = useAssistant();
  // Remembered as days, not as a place in the list, so adding a range never shifts the choice.
  const [days, setDays] = useRemembered("trends.days", 30, (v): v is number =>
    RANGES.some((r) => r.days === v),
  );
  const [tab, setTab] = useRemembered<Tab>("trends.tab", "all", isTab);
  useEffect(() => {
    if (focus) setTab(tabFor(focus));
  }, [focus, setTab]);
  // The day picked on a chart (its local midnight) and the chart's measure.
  const [picked, setPicked] = useState<{ t: number; why: string } | null>(null);
  // ...and lets go of a day picked on another (reset as the link arrives, not in an effect).
  const [focusSeen, setFocusSeen] = useState(focus);
  if (focus !== focusSeen) {
    setFocusSeen(focus);
    if (focus) setPicked(null);
  }
  const range = Math.max(
    0,
    RANGES.findIndex((r) => r.days === days),
  );
  const perfPoll = usePolled(() => api.perfDays(days), 300_000, [days]);
  // The charts draw only the range's own days; the tabs go by the last range's until they come.
  const perfDays = perfPoll?.fresh ? perfPoll.v : [];
  const trend = usePoll(() => api.diskTrend(days), 300_000, [days], { clear: true });
  const growth = usePoll(() => api.growth("/", days), 300_000, [days], { clear: true });
  // A measure the Mac never reported (no GPU, no battery controller's power) has no chart.
  // Until a new range's days come, the last range's say, so a tab doesn't blink away; before the
  // first days come, no measure is known, so no tab appears only to vanish (the remembered tab
  // stays, rather than falling back to All and back again).
  const known = perfPoll != null;
  const has = (n: string) => !!perfPoll?.v.some((d) => M[n].avg(d) != null);
  const hasGpu = has("GPU") || (!known && tab === "gpu");
  const hasPower = has("Energy") || (!known && tab === "energy");
  const shown: Tab = (tab === "gpu" && !hasGpu) || (tab === "energy" && !hasPower) ? "all" : tab;
  const tabMeasure = shown === "disk" ? null : M[TAB_CHARTS[shown][0]];
  const heatOf = tabMeasure?.hour ?? "cpu";
  // Only the measure tabs show it.
  const wantsHeat = shown !== "all" && shown !== "disk";
  const heat = usePoll(
    () => (wantsHeat ? api.byHour(heatOf) : Promise.resolve(null)),
    600_000,
    [heatOf, wantsHeat],
    {
      clear: true,
    },
  );

  const midnight = new Date();
  midnight.setHours(24, 0, 0, 0);
  const t1 = Math.max(now(), midnight.getTime() / 1000);
  const t0 = t1 - days * 86400;
  // Local midnight today, to know whether the last day recorded is today.
  const today0 = t1 - 86400;
  const xl = Array.from({ length: 5 }, (_, i) =>
    i === 4 ? "today" : dateLabel(t0 + ((t1 - t0) * i) / 4),
  );
  // A day's point sits at its middle, so a day's line runs between its neighbours'.
  const at = (d: PerfDay) => d.ts + 43_200;
  const series = (f: (d: PerfDay) => number | null): Pt[] =>
    perfDays.map((d) => ({ t: at(d), v: f(d) })).filter((p): p is Pt => p.v != null);

  // The day the side card is about: picked, else the busiest day of the tab's measure.
  // The busiest day: the highest day's figure (average, or total), not the highest peak, which
  // many days share at 100%.
  // Days with under an hour recorded (a brief wake, say) are left out of the picks.
  const full = perfDays.filter((d) => d.minutes >= 60);
  const busiestOf = (m: Measure): PerfDay | null =>
    full.reduce<PerfDay | null>(
      (b, d) => ((m.avg(d) ?? -1) > (b ? (m.avg(b) ?? -1) : -1) ? d : b),
      null,
    ) ?? (m === M.CPU ? null : busiestOf(M.CPU));
  const measure = picked ? (M[picked.why] ?? tabMeasure ?? M.CPU) : (tabMeasure ?? M.CPU);
  const day = picked
    ? (perfDays.find((d) => Math.abs(at(d) - picked.t) < 43_200) ?? null)
    : busiestOf(measure);
  const chooseTab = (id: Tab) => {
    setTab(id);
    setPicked(null);
  };

  // One measure's card: its figure now (the last day), the range in a line, and its chart.
  const chart = (m: Measure, height: number, related = false) => {
    const pts = series(m.avg);
    const band = m.peak ? series(m.peak) : undefined;
    const vals = pts.map((p) => p.v);
    const top = pts.reduce<Pt | null>((b, p) => (!b || p.v > b.v ? p : b), null);
    const max =
      m.name === "CPU" || m.name.endsWith("cores") || m.name === "GPU"
        ? 100
        : m.name === "Pressure"
          ? 4
          : m.name === "Memory"
            ? Math.max(1, ...perfDays.map((d) => d.mem_total))
            : Math.max(1, ...vals, ...(band ?? []).map((p) => p.v));
    const totals = m.avgLabel.endsWith("that day");
    const highs = m.avgLabel.includes("high");
    // The last day recorded, and the range's typical day: averaged by the minutes recorded, so
    // a day the Mac was barely on counts for little.
    const withV = perfDays.filter((d) => m.avg(d) != null);
    const last = withV[withV.length - 1];
    const lastV = last ? m.avg(last) : null;
    const mins = withV.reduce((a, d) => a + d.minutes, 0);
    const typical = mins ? withV.reduce((a, d) => a + (m.avg(d) ?? 0) * d.minutes, 0) / mins : 0;
    return (
      <section className={`card chart${related ? " related" : ""}`} key={m.name}>
        <div className="row base wrap">
          <b className="cname">
            <Labelled
              m={metricOf(
                m.name === "Read" || m.name === "Written"
                  ? "Disk I/O"
                  : m.name === "Downloaded"
                    ? "Download"
                    : m.name === "Uploaded"
                      ? "Upload"
                      : m.name,
              )}
            >
              {m.name}
            </Labelled>
          </b>
          <span className="cnow">{last ? m.fmt(lastV!) : "–"}</span>
          <span className="muted small">
            {last
              ? `${last.ts >= today0 - 3600 ? (totals ? "today so far" : "today") : dayLabel(last.ts)} · ${
                  totals
                    ? `${m.fmt(vals.reduce((a, b) => a + b, 0))} in all`
                    : highs
                      ? `the highest ${m.fmt(Math.max(...vals))}`
                      : `${m.fmt(typical)} on an average day`
                }`
              : ""}
          </span>
          <Legend
            line={
              totals
                ? `${m.name}, each day`
                : highs
                  ? `${m.name}, each day's high`
                  : `${m.name}, each day's average`
            }
            band={band ? "Each day's peak" : undefined}
          />
        </div>
        {pts.length > 1 ? (
          <Area
            pts={pts}
            hi={band}
            max={max}
            height={height}
            peak={
              top && top.v > 0
                ? { t: top.t, v: top.v, label: `${m.fmt(top.v)} · ${dateLabel(top.t)}` }
                : null
            }
            fmt={m.fmt}
            fmtT={dayLabel}
            name={m.name}
            xLabels={xl}
            domain={[t0, t1]}
            marker={day ? at(day) : undefined}
            onPick={(p) => setPicked({ t: p.t, why: m.name })}
            label={`${m.name} each day over the last ${RANGES[range].label}`}
          />
        ) : (
          <div className="empty small">A point a day: the chart appears after the second day.</div>
        )}
      </section>
    );
  };

  // The range's records for a measure: its busiest day, its highest peak, its busiest week.
  const records = (m: Measure) => {
    const best = (f: (d: PerfDay) => number | null) =>
      full.reduce<{ d: PerfDay; v: number } | null>((b, d) => {
        const v = f(d);
        return v != null && (!b || v > b.v) ? { d, v } : b;
      }, null);
    const day1 = best(m.avg);
    const peak = m.peak ? best(m.peak) : null;
    // The busiest seven days running, by the measure's daily figure.
    let week: { from: PerfDay; v: number } | null = null;
    for (let i = 6; i < perfDays.length; i++) {
      // Seven calendar days, not seven rows across a gap (an hour's slack for a clock change).
      if (perfDays[i].ts - perfDays[i - 6].ts > 6 * 86400 + 3600) continue;
      const vs = perfDays.slice(i - 6, i + 1).map(m.avg);
      if (vs.some((v) => v == null)) continue;
      const v = (vs as number[]).reduce((a, b) => a + b, 0) / 7;
      if (!week || v > week.v) week = { from: perfDays[i - 6], v };
    }
    const row = (label: string, r: { v: number; ts: number } | null, f = m.fmt) =>
      r ? (
        <button
          className="incident"
          key={label}
          onClick={() => setPicked({ t: r.ts + 43_200, why: m.name })}
          title="Show that day"
        >
          <span>
            <b>{f(r.v)}</b> <span className="muted">{label}</span>
          </span>
          <span className="muted">{dateLabel(r.ts)}</span>
        </button>
      ) : null;
    return (
      <section className="card list">
        <div className="row between head">
          <span className="eyebrow">Records · {RANGES[range].label}</span>
        </div>
        {!day1 && <div className="muted rowline">Records appear as the days are recorded.</div>}
        <div className="col gap6">
          {row(
            m.avgLabel.endsWith("that day") ? "the most in a day" : "the busiest day",
            day1 && { v: day1.v, ts: day1.d.ts },
          )}
          {row("the highest peak", peak && { v: peak.v, ts: peak.d.ts })}
          {row(
            m.avgLabel.endsWith("that day") ? "a day, the busiest week" : "the busiest week",
            week && { v: week.v, ts: week.from.ts },
          )}
        </div>
      </section>
    );
  };

  // When the tab's measure is usually busy, from the minutes kept.
  const busiest = (() => {
    if (!heat) return "";
    const cells: { d: number; h: number; v: number }[] = [];
    heat.forEach((row, d) => row.forEach((v, h) => v != null && cells.push({ d, h, v })));
    cells.sort((a, b) => b.v - a.v);
    const wd = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    const weekdays = cells.slice(0, 8).filter((c) => c.d < 5).length;
    if (cells.length <= 24 || !cells[0]) return "";
    const at = `around ${String(cells[0].h).padStart(2, "0")}:00`;
    return weekdays >= 6 ? `Busiest on weekdays ${at}.` : `Busiest on ${wd[cells[0].d]} ${at}.`;
  })();
  const heatFmt =
    heatOf === "mem"
      ? (v: number) => `${fmtMem(v)} in use on average`
      : heatOf === "disk" || heatOf === "down" || heatOf === "up"
        ? (v: number) => `${fmtRate(v)} on average`
        : heatOf === "power"
          ? (v: number) => `${fmtW(v)} on average`
          : (v: number) => `${pct(v)} on average`;
  const heatCard = (
    <section className="card">
      <div className="row between head">
        <span className="eyebrow">When it gets busy · by hour</span>
        <span className="muted small">
          the last {Math.min(historyDays, 14)} day{Math.min(historyDays, 14) === 1 ? "" : "s"}
        </span>
      </div>
      {heat ? <Heatmap grid={heat} fmt={heatFmt} /> : null}
      {busiest && <p className="muted small">{busiest}</p>}
    </section>
  );

  // ---- the disk ----
  const diskPts: Pt[] = (trend?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const first = trend?.points[0];
  const last = trend?.points[trend.points.length - 1];
  const delta = first && last ? last.used - first.used : 0;
  const spanDays = first && last ? Math.max(1, (last.ts - first.ts) / 86400) : 1;
  const perDay = delta / spanDays;
  const monthsLeft = last && perDay > 1e7 ? (last.total - last.used) / perDay / 30.4 : null;
  const big = jumps(trend?.points ?? []);
  const high = trend?.high;
  const lo = diskPts.length ? Math.min(...diskPts.map((p) => p.v)) : 0;
  const hi = diskPts.length ? Math.max(...diskPts.map((p) => p.v)) : 1;
  const pad = Math.max((hi - lo) * 0.15, 2e9);
  const rose = (growth?.landed ?? []).filter((g: Growth) => g.delta > 0).slice(0, 6);
  const diskPicked =
    picked?.why === "Disk space" && diskPts.length
      ? (trend?.points ?? []).reduce<DiskPoint | null>(
          (b, p) => (!b || Math.abs(p.ts - picked.t) < Math.abs(b.ts - picked.t) ? p : b),
          null,
        )
      : null;
  const diskCard = (height: number) => (
    <section className="card chart" key="disk">
      <div className="row base wrap">
        <b className="cname">
          <Labelled m="disk">Disk space used</Labelled>
        </b>
        <span className="cnow">{diskPts.length > 1 ? fmtDelta(delta) : "–"}</span>
        {first && last && (
          <span className="muted small">
            over {Math.round(spanDays)} days, from {fmt(first.used)} to {fmt(last.used)}
            {Math.abs(perDay) > 1e7 ? ` · about ${fmt(Math.abs(perDay))} a day` : ""}
          </span>
        )}
        <Legend line="Disk space used, each day's high" mark="Tidemark: the range's high" />
      </div>
      {diskPts.length > 1 ? (
        <Area
          pts={diskPts}
          max={hi + pad}
          min={Math.max(0, lo - pad)}
          height={height}
          mark={
            high
              ? { v: high.used, label: `Tidemark ${fmt(high.used)} · ${dateLabel(high.ts)}` }
              : null
          }
          fmt={fmt}
          fmtT={dayLabel}
          name="Disk space used"
          xLabels={xl}
          domain={[t0, t1]}
          marker={diskPicked ? diskPicked.ts : undefined}
          onPick={shown === "disk" ? (p) => setPicked({ t: p.t, why: "Disk space" }) : undefined}
          label={`Disk space used over the last ${RANGES[range].label}`}
        />
      ) : (
        <div className="empty small">
          The disk's used space is recorded every hour. The trend appears after the first day.
        </div>
      )}
      {monthsLeft != null && (
        <p className="muted small">
          At this rate the disk is full in about{" "}
          {monthsLeft >= 24
            ? `${Math.round(monthsLeft / 12)} years`
            : `${Math.max(1, Math.round(monthsLeft))} months`}
          .
        </p>
      )}
    </section>
  );
  const roseCard = (
    <section className="card list">
      <div className="row between head">
        <span className="eyebrow">Top folders that rose · {RANGES[range].label}</span>
        <button className="link small" onClick={() => go("disk")} title="Open the map (⌘4)">
          Open the map
        </button>
      </div>
      {rose.length === 0 && (
        <div className="muted rowline">
          The range compares the daily top folders. They fill in as the days pass.
        </div>
      )}
      {rose.map((r) => (
        <div className="roserow" key={r.path}>
          <span className="col minw0">
            <span className="mono ell" title={r.path}>
              {abbrev(r.path, home)}
            </span>
            <span className="muted small">
              {fmt(r.before)} → {fmt(r.size)}
            </span>
          </span>
          <FolderSpark path={r.path} days={Math.min(days, 14)} />
          <b className="r">{fmtDelta(r.delta)}</b>
        </div>
      ))}
    </section>
  );

  // ---- the side card ----
  const appsOf = (d: PerfDay, m: Measure): AppUse[] => d.apps[m.by] ?? [];
  const appV = (a: AppUse, m: Measure) =>
    m.by === "mem" ? a.mem : m.by === "energy" ? a.energy : m.by === "disk" ? (a.disk ?? 0) : a.cpu;
  const appF = (m: Measure) =>
    m.by === "mem" ? fmtMem : m.by === "energy" ? fmtW : m.by === "disk" ? fmtRate : pct;
  const side =
    shown === "disk" ? (
      <Snap>
        {diskPicked && (
          <div>
            <span className="eyebrow">Disk space</span>
            <span className="muted small"> {dayLabel(diskPicked.ts)}</span>
            <div className="snapstats">
              <div>
                <b>{fmt(diskPicked.used)}</b>
                <span className="muted small">used</span>
              </div>
              <div>
                <b>{fmt(diskPicked.total - diskPicked.used)}</b>
                <span className="muted small">free</span>
              </div>
            </div>
          </div>
        )}
        <span className="eyebrow">The biggest one-day rises</span>
        <div className="col gap6">
          {big.length === 0 && (
            <span className="muted small">None of 2 GB or more in the range.</span>
          )}
          {[...big]
            .sort((a, b) => b.ts - a.ts)
            .map((j) => (
              <button
                className="incident"
                key={j.ts}
                onClick={() => setPicked({ t: j.ts, why: "Disk space" })}
                title="Show that day"
              >
                <b>{fmtDelta(j.delta)}</b>
                <span className="muted">{dayLabel(j.ts)}</span>
              </button>
            ))}
        </div>
        {high && last && (
          <span className="muted small">
            The high-water mark was {fmt(high.used)} on {dateLabel(high.ts)}.{" "}
            {last.used < high.used - 1e9
              ? `${fmt(high.used - last.used)} freed since.`
              : "It is still near there."}
          </span>
        )}
      </Snap>
    ) : (
      <Snap>
        <div>
          <span className="eyebrow">
            {measure.what.charAt(0).toUpperCase() + measure.what.slice(1)}
          </span>
          {day && (
            <span className="muted small">
              {" "}
              {dayLabel(day.ts)}
              {!picked ? " · the range's busiest day" : ""}
            </span>
          )}
          {day ? (
            <div className="snapstats">
              <div>
                <b>{measure.avg(day) != null ? measure.fmt(measure.avg(day)!) : "–"}</b>
                <span className="muted small">{measure.avgLabel}</span>
              </div>
              {measure.peak && measure.peak(day) != null && (
                <div>
                  <b>{measure.fmt(measure.peak(day)!)}</b>
                  <span className="muted small">the day's peak</span>
                </div>
              )}
            </div>
          ) : (
            <div className="muted small">The days appear as they are recorded.</div>
          )}
        </div>
        {day && (
          <>
            {measure.note && <div className="muted small">{measure.note}</div>}
            <span className="eyebrow">The day's busiest apps</span>
            <div className="blame">
              {appsOf(day, measure).length === 0 && (
                <div className="muted small">Nothing recorded that day.</div>
              )}
              {appsOf(day, measure)
                .slice(0, 6)
                .map((a, _, all) => (
                  <div className="brow" key={a.app}>
                    <div className="row between">
                      <span className="ell">{a.app}</span>
                      <b>{appF(measure)(appV(a, measure))}</b>
                    </div>
                    <span className="bar">
                      <i
                        style={{
                          width: `${appV(all[0], measure) ? (100 * appV(a, measure)) / appV(all[0], measure) : 0}%`,
                        }}
                      />
                    </span>
                    <span className="muted small">
                      {measure.by === "mem" ? `${pct(a.cpu)} CPU` : `${fmtMem(a.mem)} memory`}
                    </span>
                  </div>
                ))}
            </div>
            <span className="muted small">
              Averaged over the minutes each app ran that day.
              {day.incidents > 0 || day.crashes > 0
                ? ` That day: ${day.incidents} incident${day.incidents === 1 ? "" : "s"}, ${day.crashes} crash${day.crashes === 1 ? "" : "es"} or hang${day.crashes === 1 ? "" : "s"}.`
                : ""}
            </span>
          </>
        )}
      </Snap>
    );

  const tabsShown = TABS.filter(
    (t) => (t.id !== "gpu" || hasGpu) && (t.id !== "energy" || hasPower),
  );
  const panel = (id: Tab) => ({
    role: "tabpanel",
    id: `ttab-${id}-panel`,
    "aria-labelledby": `ttab-${id}`,
  });

  return (
    <>
      {bar(
        <>
          <span className="ttl">Trends</span>
          <div className="seg" role="group" aria-label="Time range">
            {RANGES.map((r, i) => (
              <button
                key={r.label}
                className={i === range ? "on" : ""}
                aria-pressed={i === range}
                onClick={() => {
                  setDays(RANGES[i].days);
                  setPicked(null);
                }}
                title={`Every measure over the last ${r.label}`}
              >
                {r.label}
              </button>
            ))}
          </div>
          <div className="grow" />
        </>,
      )}
      <div className="page perf">
        <div className="ptabs" role="tablist" aria-label="Measures">
          {tabsShown.map((t, i) => (
            <button
              key={t.id}
              id={`ttab-${t.id}`}
              role="tab"
              aria-selected={shown === t.id}
              aria-controls={shown === t.id ? `ttab-${t.id}-panel` : undefined}
              tabIndex={shown === t.id ? 0 : -1}
              className={shown === t.id ? "on" : ""}
              title={TIPS[t.id]}
              onClick={() => chooseTab(t.id)}
              onKeyDown={(e) => {
                const n = tabsShown.length;
                const to =
                  e.key === "ArrowRight"
                    ? (i + 1) % n
                    : e.key === "ArrowLeft"
                      ? (i - 1 + n) % n
                      : e.key === "Home"
                        ? 0
                        : e.key === "End"
                          ? n - 1
                          : -1;
                if (to < 0) return;
                e.preventDefault();
                chooseTab(tabsShown[to].id);
                document.getElementById(`ttab-${tabsShown[to].id}`)?.focus();
              }}
            >
              <span className="labelled">
                {t.m ? <MetricIcon m={t.m} /> : <AllIcon />}
                {t.label}
              </span>
            </button>
          ))}
        </div>
        <div className="perfmain" {...panel(shown)}>
          {shown === "disk" ? (
            <>
              {diskCard(260)}
              {roseCard}
            </>
          ) : (
            <>
              {TAB_CHARTS[shown]
                // Before the first days come, every chart stays (empty) rather than the panel
                // emptying and filling again; only the tabs wait to know.
                .filter((n) => !known || has(n) || n === "CPU")
                .map((n, i) => chart(M[n], shown !== "all" && i === 0 ? 220 : 170))}
              {TAB_ALSO[shown] && TAB_ALSO[shown].charts.some((n) => !known || has(n)) && (
                <>
                  <Alongside why={TAB_ALSO[shown].why} />
                  {TAB_ALSO[shown].charts
                    .filter((n) => !known || has(n))
                    .map((n) => chart(M[n], 170, true))}
                </>
              )}
              {shown === "all" ? (
                diskCard(170)
              ) : (
                <div className="cards2">
                  {tabMeasure && records(tabMeasure)}
                  {heatCard}
                </div>
              )}
            </>
          )}
          <section className="askstrip" title={assistant.reason ?? undefined}>
            <span className="lbl">Ask Tidemark</span>
            {[
              "Will I run out of disk this year?",
              `What grew most in the last ${RANGES[range].label}?`,
              `When is my ${MACHINE} usually busiest, and why?`,
            ].map((q) => (
              <button
                className="chip"
                key={q}
                disabled={!assistant.available}
                onClick={() => ask(q)}
                title="Ask this (⌘K opens Ask)"
              >
                {q}
              </button>
            ))}
          </section>
        </div>
        {side}
      </div>
    </>
  );
}
