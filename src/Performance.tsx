// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Recent (the "performance" screen): CPU and memory over a range with the week's tidemark and the range's peak,
// smaller tiles for swap, disk, network and heat, and what every app was doing at a chosen
// moment (the peak, or wherever the chart is clicked).

import { useAssistant } from "./assistant";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { AllIcon, Labelled, MetricIcon } from "./icons";
import { metricOf } from "./metrics";
import type { AskDetail } from "./Ask";
import { AskButton } from "./AskButton";
import { flaggedDetail } from "./askDetails";
import { flagFor } from "./status";
import { api } from "./api";
import { Area, Legend, Spark, type Pt } from "./charts";
import { crashGroups, crashTitle, incidentTitle } from "./incidents";
import { gapsOf, now, upToNow, usePoll } from "./perf";
import { isTab, TABS, tabFor, type Tab } from "./perfTabs";
import { index, useRemembered } from "./remember";
import type { AppUse, Incident, Live, Minute } from "./types";
import { batteryLine, bucketWord, fmt, fmtDelta, fmtAt, fmtMins, fmtRate, fmtW, pct } from "./util";

const RANGES = [
  { label: "1 hour", secs: 3600 },
  { label: "24 hours", secs: 86400 },
  { label: "7 days", secs: 7 * 86400 },
  { label: "14 days", secs: 14 * 86400 },
];

function xLabels(secs: number, n = 6): string[] {
  const t1 = now(),
    t0 = t1 - secs;
  return Array.from({ length: n + 1 }, (_, i) => {
    if (i === n) return "now";
    const d = new Date((t0 + ((t1 - t0) * i) / n) * 1000);
    return secs > 2 * 86400
      ? d.toLocaleDateString("en-GB", { weekday: "short", day: "numeric" })
      : d.toTimeString().slice(0, 5);
  });
}

function peakOf(points: Minute[], k: "cpu_max" | "mem_max"): { t: number; v: number } | null {
  let best: Minute | null = null;
  for (const m of points) if (!best || m[k] > best[k]) best = m;
  return best ? { t: best.ts, v: best[k] } : null;
}

const THERMAL = ["Nominal", "Fair", "Serious", "Critical"];

/// One chart card's measure: what Performance draws for CPU, memory, a direction of traffic...
interface ChartSpec {
  name: string;
  /// The Details target it answers to, when not its name.
  focus?: string;
  now: string;
  sub: string;
  /// A shorter line for the All tab, where the details are left to the measure's tab.
  brief?: string;
  pts: Pt[];
  band?: Pt[];
  max: number;
  fmt: (v: number) => string;
  mark: { v: number; label: string } | null;
  peak: { t: number; v: number; label: string } | null;
  extra: (i: number) => ReactNode;
  also?: { pts: Pt[]; label: string; dash: string }[];
}

const PRESSURE = (v: number) => (v >= 4 ? "Critical" : v >= 2 ? "Raised" : "Normal");

/// What the snapshot says about a chart's measure at a moment: the average the line shows, the
/// moment's peak where there is one, and which apps to list. macOS splits CPU, memory, energy
/// and disk by app; for the GPU and the network the apps are the busiest by CPU, and the
/// snapshot says so.
interface Measure {
  what: string;
  avg: (m: Minute) => number | null;
  peak?: (m: Minute) => number | null;
  fmt: (v: number) => string;
  by: "cpu" | "mem" | "energy" | "disk";
  note?: string;
  /// What a chart point holds, where it isn't an average ("highest" for pressure).
  avgLabel?: string;
}
const BY_CPU = (what: string) =>
  `macOS doesn't say which apps use the ${what}: these were the busiest by CPU then.`;
const MEASURES: Record<string, Measure> = {
  CPU: { what: "CPU", avg: (m) => m.cpu, peak: (m) => m.cpu_max, fmt: pct, by: "cpu" },
  "Performance cores": { what: "performance cores", avg: (m) => m.cpu_p, fmt: pct, by: "cpu" },
  "Efficiency cores": { what: "efficiency cores", avg: (m) => m.cpu_e, fmt: pct, by: "cpu" },
  GPU: {
    what: "GPU",
    avg: (m) => m.gpu,
    peak: (m) => m.gpu_max,
    fmt: pct,
    by: "cpu",
    note: BY_CPU("GPU"),
  },
  Memory: { what: "memory in use", avg: (m) => m.mem, peak: (m) => m.mem_max, fmt, by: "mem" },
  "App memory": { what: "app memory", avg: (m) => m.mem_app, fmt, by: "mem" },
  Wired: { what: "wired memory", avg: (m) => m.mem_wired, fmt, by: "mem" },
  Compressed: { what: "compressed memory", avg: (m) => m.mem_comp, fmt, by: "mem" },
  Swap: { what: "swap", avg: (m) => m.swap, peak: (m) => m.swap_max, fmt, by: "mem" },
  Pressure: {
    what: "memory pressure",
    avg: (m) => m.pressure,
    fmt: PRESSURE,
    by: "mem",
    avgLabel: "highest",
  },
  "Disk I/O": {
    what: "disk reads and writes",
    avg: (m) => m.disk_r + m.disk_w,
    peak: (m) => m.disk_max,
    fmt: fmtRate,
    by: "disk",
  },
  Reads: {
    what: "disk reads",
    avg: (m) => m.disk_r,
    fmt: fmtRate,
    by: "disk",
    note: "The apps are listed by what they read and wrote together.",
  },
  Writes: {
    what: "disk writes",
    avg: (m) => m.disk_w,
    fmt: fmtRate,
    by: "disk",
    note: "The apps are listed by what they read and wrote together.",
  },
  Download: {
    what: "download",
    avg: (m) => m.net_in,
    peak: (m) => m.net_in_max,
    fmt: fmtRate,
    by: "cpu",
    note: BY_CPU("network"),
  },
  Upload: {
    what: "upload",
    avg: (m) => m.net_out,
    peak: (m) => m.net_out_max,
    fmt: fmtRate,
    by: "cpu",
    note: BY_CPU("network"),
  },
  Energy: {
    what: "power drawn",
    avg: (m) => m.power,
    peak: (m) => m.power_max,
    fmt: fmtW,
    by: "energy",
  },
};
MEASURES["Disk reads"] = MEASURES.Reads;
MEASURES["Disk writes"] = MEASURES.Writes;
MEASURES.Power = MEASURES.Energy;

export default function Performance({
  live,
  historyDays,
  bar,
  ask,
  focus,
}: {
  live: Live | null;
  historyDays: number;
  bar: (left: ReactNode) => ReactNode;
  ask: (q: string, detail?: AskDetail) => void;
  /// A Details link's target: its tab opens, so the section is there to scroll to.
  focus?: string;
}) {
  const assistant = useAssistant();
  const [tab, setTab] = useRemembered<Tab>("performance.tab", "all", isTab);
  useEffect(() => {
    if (focus) setTab(tabFor(focus));
  }, [focus, setTab]);
  const [chosen, setRange] = useRemembered("performance.range", 1, index(RANGES.length));
  // Ranges longer than the history kept are not offered; a remembered one is held to what is.
  const allowed = RANGES.filter(
    (r) => r.secs <= Math.max(historyDays, 1) * 86400 || r.secs <= 86400,
  );
  const range = Math.min(chosen, allowed.length - 1);
  const secs = RANGES[range].secs;
  const hist = usePoll(() => api.perf(secs, 480), 60_000, [secs]);
  const incidents = usePoll(() => api.incidents(14), 60_000);
  // The moment the snapshot is of, and the chart it was picked on (a MEASURES name); else the
  // range's CPU peak.
  // `mode`: a point on a line (the apps' averages then), a peak (each app at its busiest then),
  // or an incident (its own snapshot, taken at its peak).
  const [picked, setPicked] = useState<{
    t: number;
    why: string;
    mode: "line" | "peak" | "incident";
    inc?: Incident;
  } | null>(null);
  const pick = (why: string, t: number, atPeak = false) =>
    setPicked({ t, why, mode: atPeak ? "peak" : "line" });
  const points = useMemo(() => hist?.points ?? [], [hist]);
  // What one point stands for, as the history folds the range (perf_history's 480 points).
  const bucket = Math.max(60, Math.floor(secs / 480));
  const per = bucketWord(bucket);
  const cpuPeak = peakOf(points, "cpu_max");
  const memPeak = peakOf(points, "mem_max");
  // On the Memory tab the snapshot starts at the memory peak; elsewhere at the CPU's.
  const byMemTab = tab === "memory" && memPeak;
  // A moment picked earlier that the range no longer reaches (the hour moved on) is let go.
  const pickedIn = picked && picked.t >= now() - secs ? picked : null;
  const at =
    pickedIn ??
    (byMemTab
      ? { t: memPeak.t, why: "Memory", mode: "peak" as const }
      : cpuPeak
        ? { t: cpuPeak.t, why: "CPU", mode: "peak" as const }
        : null);
  // The range's energy: what used the most, and the times the Mac slept.
  // Only the tabs with a range's busiest apps ask for them: eight queries over the range.
  const wantsRange = ["cpu", "memory", "diskio", "energy"].includes(tab);
  const rangeApps = usePoll(
    () => (wantsRange ? api.appsBetween(now() - secs, now()) : Promise.resolve(null)),
    60_000,
    [secs, wantsRange],
  );
  const crashList = usePoll(() => api.crashes(14), 300_000);
  // Disk space is recorded hourly, so a range under a day shows the last day.
  const diskSecs = Math.max(secs, 86400);
  const diskDays = Math.ceil(diskSecs / 86400);
  const diskRange = usePoll(() => api.diskTrend(diskDays), 300_000, [diskDays]);
  const diskWeek = usePoll(() => api.diskTrend(7), 300_000);
  const sleeps = usePoll(() => api.sleeps(Math.max(1, Math.ceil(secs / 86400))), 60_000, [secs]);
  const latest = live?.samples[live.samples.length - 1];
  // Where the record has holes in this range, and why.
  const gaps = gapsOf(
    points.map((m) => m.ts),
    [now() - secs, now()],
    sleeps ?? [],
  );
  const total = live?.mem_total || points[0]?.mem_total || 1;
  const tm = hist?.tidemarks ?? {};

  // The minute's numeric measures (the ones a chart can draw).
  type Num = { [K in keyof Minute]: Minute[K] extends number ? K : never }[keyof Minute];
  // Each history measure's live counterpart, so the charts run on to the latest reading.
  const LIVE: Partial<
    Record<Num, "cpu" | "mem" | "net_in" | "net_out" | "swap" | "disk_r" | "disk_w">
  > = {
    cpu: "cpu",
    cpu_max: "cpu",
    mem: "mem",
    mem_max: "mem",
    net_in: "net_in",
    net_out: "net_out",
    swap: "swap",
    disk_r: "disk_r",
    disk_w: "disk_w",
  };
  const ser = (k: Num): Pt[] => {
    const lk = LIVE[k];
    return upToNow(
      points.map((m) => ({ t: m.ts, v: m[k] })),
      latest ? latest.t / 1000 : undefined,
      lk && latest ? latest[lk] : undefined,
    );
  };
  // The chart point the moment falls in: within a point's span, or none (no history there).
  const atPoint = points.reduce<Minute | null>(
    (b, m) =>
      at && Math.abs(m.ts - at.t) <= bucket && (!b || Math.abs(m.ts - at.t) < Math.abs(b.ts - at.t))
        ? m
        : b,
    null,
  );
  // The moment's span: the minutes its chart point stands for.
  const span0 = Math.floor(atPoint?.ts ?? at?.t ?? 0);
  const span1 = span0 + bucket - 1;
  const apps = usePoll(
    () =>
      at && atPoint && at.mode !== "incident"
        ? api.appsBetween(span0, span1)
        : Promise.resolve(null),
    60_000,
    [span0, span1, at?.mode, !!atPoint],
  );
  const measure = MEASURES[at?.why ?? "CPU"] ?? MEASURES.CPU;
  const mode = at?.mode ?? "peak";
  const appV = (a: AppUse) => {
    const k = measure.by;
    return (mode === "peak" ? a[`${k}_max`] : a[k]) ?? 0;
  };
  const blame =
    mode === "incident"
      ? [...(picked?.inc?.snapshot ?? [])].sort((a, b) => appV(b) - appV(a))
      : ((mode === "peak" ? apps?.[`peak_${measure.by}`] : apps?.[`by_${measure.by}`]) ?? []);
  const top = blame[0];
  const hhmm = (t: number) => new Date(t * 1000).toTimeString().slice(0, 5);
  const spanText = bucket > 60 ? `${hhmm(span0)}–${hhmm(span1 + 1)}` : hhmm(span0);
  // What the list is, so it can't be read as something else.
  const listLabel =
    mode === "incident"
      ? `At the incident's peak, ${hhmm(at?.t ?? 0)}`
      : mode === "peak"
        ? `Each app at its busiest, ${spanText}`
        : `Each app's average, ${spanText}`;
  // The moment's figures: the average the line shows, and the peak inside it.
  const atAvg = atPoint ? measure.avg(atPoint) : null;
  const atPeak = atPoint && measure.peak ? measure.peak(atPoint) : null;
  const appF =
    measure.by === "mem"
      ? fmt
      : measure.by === "energy"
        ? fmtW
        : measure.by === "disk"
          ? fmtRate
          : pct;
  // The CPU's performance and efficiency cores, where the Mac says which is which.
  const kindLine = (k: "cpu_p" | "cpu_e"): Pt[] =>
    upToNow(
      points.filter((m) => m[k] != null).map((m) => ({ t: m.ts, v: m[k] as number })),
      latest ? latest.t / 1000 : undefined,
      latest?.[k] ?? undefined,
    );
  const cpuKinds =
    points.some((m) => m.cpu_p != null) || latest?.cpu_p != null
      ? [
          { pts: kindLine("cpu_p"), label: "Performance cores", dash: "5 3" },
          { pts: kindLine("cpu_e"), label: "Efficiency cores", dash: "1.5 2.5" },
        ]
      : undefined;
  const charts = [
    {
      name: "CPU",
      now: latest ? pct(latest.cpu) : "–",
      sub: `${live?.cores ?? "–"} cores${points.length ? ` · average ${pct(points.reduce((s, m) => s + m.cpu, 0) / points.length)}` : ""}`,
      pts: ser("cpu"),
      band: ser("cpu_max"),
      max: 100,
      fmt: pct,
      mark: tm.cpu
        ? { v: tm.cpu.value, label: `Tidemark ${pct(tm.cpu.value)} · ${fmtAt(tm.cpu.at)}` }
        : null,
      peak: cpuPeak ? { ...cpuPeak, label: `${pct(cpuPeak.v)} · ${fmtAt(cpuPeak.t)}` } : null,
      extra: (i: number) => {
        const m = points[i] ?? latest;
        return (
          <>
            {points[i] ? <span>Peak {pct(points[i].cpu_max)}</span> : <span>Now</span>}
            {m?.cpu_p != null && m.cpu_e != null && (
              <span>
                Performance {pct(m.cpu_p)} · efficiency {pct(m.cpu_e)}
              </span>
            )}
          </>
        );
      },
      also: cpuKinds,
    },
    // The GPU, where one reports how busy it is.
    ...(points.some((m) => m.gpu != null) || latest?.gpu != null
      ? [
          {
            name: "GPU",
            now: latest?.gpu != null ? pct(latest.gpu) : "–",
            sub: "graphics, video and machine learning",
            pts: upToNow(
              points.filter((m) => m.gpu != null).map((m) => ({ t: m.ts, v: m.gpu as number })),
              latest ? latest.t / 1000 : undefined,
              latest?.gpu,
            ),
            band: upToNow(
              points
                .filter((m) => m.gpu_max != null)
                .map((m) => ({ t: m.ts, v: m.gpu_max as number })),
              latest ? latest.t / 1000 : undefined,
              latest?.gpu,
            ),
            max: 100,
            fmt: pct,
            mark: tm.gpu
              ? { v: tm.gpu.value, label: `Tidemark ${pct(tm.gpu.value)} · ${fmtAt(tm.gpu.at)}` }
              : null,
            peak: (() => {
              const m = points.reduce<Minute | null>(
                (b, x) => ((x.gpu_max ?? -1) > (b?.gpu_max ?? -1) ? x : b),
                null,
              );
              return m?.gpu_max != null
                ? { t: m.ts, v: m.gpu_max, label: `${pct(m.gpu_max)} · ${fmtAt(m.ts)}` }
                : null;
            })(),
            extra: () => null,
          },
        ]
      : []),
    {
      name: "Memory",
      now: latest ? fmt(latest.mem) : "–",
      sub:
        `${total > 1 ? `of ${fmt(total)} · ` : ""}pressure ${latest && latest.pressure >= 4 ? "critical" : latest && latest.pressure >= 2 ? "raised" : "normal"}` +
        (latest?.mem_parts
          ? ` · apps ${fmt(latest.mem_parts.app)}, wired ${fmt(latest.mem_parts.wired)}, compressed ${fmt(latest.mem_parts.compressed)}`
          : ""),
      brief: `${total > 1 ? `of ${fmt(total)} · ` : ""}pressure ${latest && latest.pressure >= 4 ? "critical" : latest && latest.pressure >= 2 ? "raised" : "normal"}`,
      pts: ser("mem"),
      band: ser("mem_max"),
      max: total,
      fmt,
      mark: tm.mem
        ? { v: tm.mem.value, label: `Tidemark ${fmt(tm.mem.value)} · ${fmtAt(tm.mem.at)}` }
        : null,
      peak: memPeak ? { ...memPeak, label: `${fmt(memPeak.v)} · ${fmtAt(memPeak.t)}` } : null,
      extra: (i: number) => {
        const m = points[i];
        if (!m) return <span>Now</span>;
        return (
          <>
            <span>
              Peak {fmt(m.mem_max)} · swap {fmt(m.swap_max)}
            </span>
            {m.mem_app != null && (
              <span>
                Apps {fmt(m.mem_app)} · wired {fmt(m.mem_wired ?? 0)} · compressed{" "}
                {fmt(m.mem_comp ?? 0)}
              </span>
            )}
          </>
        );
      },
    },
  ];

  // Network: download and upload as two small charts on one scale (not one chart with two axes),
  // what moved in the range, and each direction's peak.
  type Way = "net_in" | "net_out" | "disk_r" | "disk_w";
  const moved = (k: Way) => points.reduce((s, m) => s + m[k] * (m.minutes || 1) * 60, 0);
  const netTop = Math.max(1, ...points.map((m) => Math.max(m.net_in, m.net_out)));
  const netPeak = (k: Way) => {
    const m = points.reduce<Minute | null>((b, x) => (!b || x[k] > b[k] ? x : b), null);
    return m && m[k] > 0 ? { t: m.ts, v: m[k], label: `${fmtRate(m[k])} · ${fmtAt(m.ts)}` } : null;
  };
  // Disk I/O: reads and writes, the same way as the network.
  const ioTop = Math.max(1, ...points.map((m) => Math.max(m.disk_r, m.disk_w)));
  const io = [
    { name: "Reads", k: "disk_r" as const, now: latest?.disk_r, total: moved("disk_r") },
    { name: "Writes", k: "disk_w" as const, now: latest?.disk_w, total: moved("disk_w") },
  ];
  const net = [
    { name: "Download", k: "net_in" as const, now: latest?.net_in, total: moved("net_in") },
    { name: "Upload", k: "net_out" as const, now: latest?.net_out, total: moved("net_out") },
  ];

  // One measure's chart card (CPU, GPU, memory): its figure, flag, legend and chart.
  // `extras` draws the parts of the whole too (the CPU's kinds of core); the All tab leaves them
  // to the measure's own tab.
  const chartCard = (c: ChartSpec, height = 170, extras = true) => (
    <section className="card chart" key={c.name} data-focus={c.focus ?? c.name.toLowerCase()}>
      <div className="row base wrap">
        <b className="cname">
          <Labelled m={metricOf(c.name)}>{c.name}</Labelled>
        </b>
        <span
          className={`cnow${flagFor(live?.status, c.name) ? ` ${flagFor(live?.status, c.name)?.cls}` : ""}`}
        >
          {c.now}
        </span>
        {flagFor(live?.status, c.name) && (
          <span className={`badge ${flagFor(live?.status, c.name)?.tone}`}>
            {flagFor(live?.status, c.name)?.label}
          </span>
        )}
        {flagFor(live?.status, c.name) && (
          <AskButton
            title={`Ask why ${c.name.toLowerCase()} is flagged (opens Ask with this chart's figures)`}
            onAsk={() => {
              const vals = c.pts.map((p) => p.v);
              const { question, detail } = flaggedDetail(live, c.name, [
                `Now: ${c.now}, ${c.sub}.`,
                vals.length
                  ? `Over the last ${RANGES[range].label}: lowest ${c.fmt(Math.min(...vals))}, highest ${c.fmt(Math.max(...vals))}, average ${c.fmt(vals.reduce((a, b) => a + b, 0) / vals.length)}.`
                  : "",
                c.peak ? `The range's peak: ${c.peak.label}.` : "",
                c.mark ? `The week's ${c.mark.label.toLowerCase()}.` : "",
              ]);
              ask(question, detail);
            }}
          />
        )}
        <span className="muted small">{extras ? c.sub : (c.brief ?? c.sub)}</span>
        <span className="grow" />
        <Legend
          line={`${c.name}, ${per} average`}
          band={
            c.band?.length
              ? per === "minute"
                ? "Each minute's peak"
                : `Each ${per} peak`
              : undefined
          }
          mark={c.mark ? "Week's tidemark" : undefined}
          also={extras ? c.also : undefined}
          gaps={gapsOf(
            c.pts.map((p) => p.t),
            [now() - secs, now()],
            sleeps ?? [],
          )}
        />
      </div>
      {c.pts.length ? (
        <Area
          height={height}
          tone={flagFor(live?.status, c.name)?.tone}
          marker={at?.t}
          pts={c.pts}
          hi={c.band}
          max={c.max}
          mark={c.mark}
          peak={c.peak}
          fmt={c.fmt}
          fmtT={(t) => fmtAt(t)}
          name={c.name}
          extra={c.extra}
          also={extras ? c.also : undefined}
          xLabels={xLabels(secs)}
          domain={[now() - secs, now()]}
          sleeps={sleeps ?? []}
          label={`${c.name} over the last ${RANGES[range].label}`}
          onPick={(p, atPeak) => pick(c.name, p.t, atPeak)}
        />
      ) : (
        <div className="empty small">Recording… the first minute appears shortly.</div>
      )}
    </section>
  );

  // The disk's, the network's and the energy's cards, with their charts' height.
  const ioCard = (h = 100) => (
    <section className="card chart" data-focus="disk i/o">
      <div className="row base wrap">
        <b className="cname">
          <Labelled m="diskio">Disk I/O</Labelled>
        </b>
        <span className="cnow">
          {latest ? `${fmtRate(latest.disk_r)} read · ${fmtRate(latest.disk_w)} written` : "–"}
        </span>
        <span className="muted small">
          {points.length
            ? `${fmt(io[0].total)} read, ${fmt(io[1].total)} written in the last ${RANGES[range].label}`
            : ""}
        </span>
        <span className="grow" />
        <Legend line={`Average per ${per.replace("-minute", " minutes")}, each way`} gaps={gaps} />
      </div>
      <div className="netpair">
        {io.map((n) => (
          <div className="minw0" key={n.k}>
            <div className="row between small">
              <span className="eyebrow">
                <Labelled m="diskio">{n.name}</Labelled>
              </span>
              <span className="muted">{n.now != null ? `${fmtRate(n.now)} now` : ""}</span>
            </div>
            {points.length ? (
              <Area
                marker={at?.t}
                onPick={(p, atPeak) => pick(n.name, p.t, atPeak)}
                pts={ser(n.k)}
                max={ioTop}
                height={h}
                peak={netPeak(n.k)}
                domain={[now() - secs, now()]}
                sleeps={sleeps ?? []}
                xLabels={xLabels(secs, 3)}
                fmt={fmtRate}
                fmtT={(t) => fmtAt(t)}
                name={n.name}
                label={`Disk ${n.name.toLowerCase()} over the last ${RANGES[range].label}`}
              />
            ) : (
              <div className="empty small">Recording…</div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
  const netCard = (h = 100) => (
    <section className="card chart" data-focus="network">
      <div className="row base wrap">
        <b className="cname">
          <Labelled m="network">Network</Labelled>
        </b>
        <span className="cnow">
          {latest ? `↓ ${fmtRate(latest.net_in)}  ↑ ${fmtRate(latest.net_out)}` : "–"}
        </span>
        <span className="muted small">
          {points.length
            ? `${fmt(net[0].total)} down, ${fmt(net[1].total)} up in the last ${RANGES[range].label}`
            : ""}
        </span>
        <span className="grow" />
        <Legend line={`Average per ${per.replace("-minute", " minutes")}, each way`} gaps={gaps} />
      </div>
      <div className="netpair">
        {net.map((n) => (
          <div className="minw0" key={n.k}>
            <div className="row between small">
              <span className="eyebrow">
                <Labelled m={metricOf(n.name)}>{n.name}</Labelled>
              </span>
              <span className="muted">{n.now != null ? `${fmtRate(n.now)} now` : ""}</span>
            </div>
            {points.length ? (
              <Area
                marker={at?.t}
                onPick={(p, atPeak) => pick(n.name, p.t, atPeak)}
                pts={ser(n.k)}
                max={netTop}
                height={h}
                peak={netPeak(n.k)}
                domain={[now() - secs, now()]}
                sleeps={sleeps ?? []}
                xLabels={xLabels(secs, 3)}
                fmt={fmtRate}
                fmtT={(t) => fmtAt(t)}
                name={n.name}
                label={`${n.name} over the last ${RANGES[range].label}`}
              />
            ) : (
              <div className="empty small">Recording…</div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
  const energyCard = (h = 120) => (
    <section className="card chart" data-focus="energy">
      <div className="row base wrap">
        <b className="cname">
          <Labelled m="energy">Energy</Labelled>
        </b>
        <span
          className={`cnow${flagFor(live?.status, "Energy") ? ` ${flagFor(live?.status, "Energy")?.cls}` : ""}`}
        >
          {latest?.power != null ? fmtW(latest.power) : "–"}
        </span>
        {flagFor(live?.status, "Energy") && (
          <span className={`badge ${flagFor(live?.status, "Energy")?.tone}`}>
            {flagFor(live?.status, "Energy")?.label}
          </span>
        )}
        {flagFor(live?.status, "Energy") && (
          <AskButton
            title="Ask why energy is flagged (opens Ask with this chart's figures)"
            onAsk={() => {
              const { question, detail } = flaggedDetail(live, "Energy", [
                `Power drawn now: ${latest?.power != null ? fmtW(latest.power) : "unknown"}.`,
                live?.battery
                  ? `Battery: ${batteryLine(live.battery)}.`
                  : "No battery on this Mac.",
              ]);
              ask(question, detail);
            }}
          />
        )}
        <span className="muted small">
          {live?.battery
            ? `battery ${batteryLine(live.battery)}`
            : live
              ? "no battery on this Mac: apps only"
              : ""}
        </span>
        <span className="grow" />
        <Legend
          line={`Power, ${per} average`}
          band={per === "minute" ? "Each minute's peak" : `Each ${per} peak`}
          mark="Week's tidemark"
          gaps={gapsOf(
            points.filter((m) => m.power != null).map((m) => m.ts),
            [now() - secs, now()],
            sleeps ?? [],
          )}
        />
      </div>
      <div className="energygrid">
        <div className="minw0">
          {points.some((m) => m.power != null) ? (
            <Area
              marker={at?.t}
              onPick={(p, atPeak) => pick("Power", p.t, atPeak)}
              pts={upToNow(
                points
                  .filter((m) => m.power != null)
                  .map((m) => ({ t: m.ts, v: m.power as number })),
                latest ? latest.t / 1000 : undefined,
                latest?.power,
              )}
              hi={upToNow(
                points
                  .filter((m) => m.power_max != null)
                  .map((m) => ({ t: m.ts, v: m.power_max as number })),
                latest ? latest.t / 1000 : undefined,
                latest?.power,
              )}
              max={Math.max(1, ...points.map((m) => m.power_max ?? 0))}
              height={h}
              mark={
                tm.power
                  ? {
                      v: tm.power.value,
                      label: `Tidemark ${fmtW(tm.power.value)} · ${fmtAt(tm.power.at)}`,
                    }
                  : null
              }
              peak={(() => {
                const m = points.reduce<Minute | null>(
                  (b, x) => ((x.power_max ?? 0) > (b?.power_max ?? 0) ? x : b),
                  null,
                );
                return m?.power_max
                  ? {
                      t: m.ts,
                      v: m.power_max,
                      label: `${fmtW(m.power_max)} · ${fmtAt(m.ts)}`,
                    }
                  : null;
              })()}
              fmt={fmtW}
              fmtT={(t) => fmtAt(t)}
              name="Power"
              domain={[now() - secs, now()]}
              sleeps={sleeps ?? []}
              xLabels={xLabels(secs, 4)}
              label={`Power drawn over the last ${RANGES[range].label}`}
            />
          ) : (
            <div className="empty small">
              {live?.battery
                ? "Recording… the first minute appears shortly."
                : "This Mac reports no power draw."}
            </div>
          )}
        </div>
        <div className="estats">
          {live?.battery && (
            <>
              <div>
                <span className="eyebrow">
                  <Labelled m="battery">Battery</Labelled>
                </span>
                <b className="tv">{Math.round(live.battery.percent)}%</b>
                <Spark
                  pts={points
                    .filter((m) => m.battery != null)
                    .map((m) => ({ t: m.ts, v: m.battery as number }))}
                  min={0}
                  max={100}
                  width={160}
                  height={24}
                  sleeps={sleeps ?? []}
                  domain={[now() - secs, now()]}
                  label="Battery charge over the range"
                />
                <span className="muted small">{batteryLine(live.battery)}</span>
              </div>
              {live.battery.health != null && (
                <div>
                  <span className="eyebrow">Battery health</span>
                  <b className="tv">{Math.round(live.battery.health)}%</b>
                  <span className="muted small">
                    of its design capacity
                    {live.battery.cycles != null ? ` · ${live.battery.cycles} cycles` : ""}
                  </span>
                </div>
              )}
            </>
          )}
          {(() => {
            const sl = (sleeps ?? []).find(
              (x) => x.battery_before != null && x.battery_after != null,
            );
            if (!sl) return null;
            const used = (sl.battery_before ?? 0) - (sl.battery_after ?? 0);
            return (
              <div>
                <span className="eyebrow">Last sleep</span>
                <b className="tv">{fmtMins(Math.round((sl.ended - sl.started) / 60))}</b>
                <span className="muted small">
                  {used > 0.5
                    ? `used ${Math.round(used)}% of the battery`
                    : "used next to no battery"}{" "}
                  · {fmtAt(sl.started)}
                </span>
              </div>
            );
          })()}
        </div>
      </div>
      {(rangeApps?.by_energy ?? []).some((a) => a.energy > 0) && (
        <div className="eapps">
          <span className="eyebrow">Using the most energy · {RANGES[range].label}</span>
          {(rangeApps?.by_energy ?? [])
            .filter((a) => a.energy > 0)
            .slice(0, 5)
            .map((a, _, all) => (
              <div className="brow" key={a.app}>
                <div className="row between">
                  <span className="ell">{a.app}</span>
                  <b>
                    {Math.round(
                      (100 * a.energy) /
                        Math.max(
                          1e-9,
                          (rangeApps?.by_energy ?? []).reduce((t, x) => t + x.energy, 0),
                        ),
                    )}
                    %
                  </b>
                </div>
                <span className="bar">
                  <i style={{ width: `${(100 * a.energy) / all[0].energy}%` }} />
                </span>
              </div>
            ))}
          <span className="muted small">
            Each app's share of the energy the busiest eight used. The display and macOS itself use
            more, and only your own apps can be measured.
          </span>
        </div>
      )}
    </section>
  );

  // The All tab: one chart a measure, each the same size, the pairs split.
  const peakIn = (of: (m: Minute) => number, f: (v: number) => string) => {
    const m = points.reduce<Minute | null>((b, x) => (!b || of(x) > of(b) ? x : b), null);
    return m && of(m) > 0 ? { t: m.ts, v: of(m), label: `${f(of(m))} · ${fmtAt(m.ts)}` } : null;
  };
  const inRange = ` in the last ${RANGES[range].label}`;
  const allMore = (): ChartSpec[] => [
    ...io.map((n) => ({
      name: `Disk ${n.name.toLowerCase()}`,
      focus: "disk i/o",
      now: n.now != null ? fmtRate(n.now) : "–",
      sub: points.length
        ? `${fmt(n.total)} ${n.k === "disk_r" ? "read" : "written"}${inRange}`
        : "",
      pts: ser(n.k),
      max: ioTop,
      fmt: fmtRate,
      mark: null,
      peak: peakIn((m) => m[n.k], fmtRate),
      extra: () => null,
    })),
    ...net.map((n) => ({
      name: n.name,
      focus: "network",
      now: n.now != null ? fmtRate(n.now) : "–",
      sub: points.length ? `${fmt(n.total)} ${n.k === "net_in" ? "down" : "up"}${inRange}` : "",
      pts: ser(n.k),
      band: ser(n.k === "net_in" ? "net_in_max" : "net_out_max"),
      max: netTop,
      fmt: fmtRate,
      mark: null,
      peak: peakIn((m) => m[n.k], fmtRate),
      extra: () => null,
    })),
    ...(points.some((m) => m.power != null) || latest?.power != null
      ? [
          {
            name: "Energy",
            now: latest?.power != null ? fmtW(latest.power) : "–",
            sub: live?.battery ? `battery ${batteryLine(live.battery)}` : "power drawn",
            pts: upToNow(
              points.filter((m) => m.power != null).map((m) => ({ t: m.ts, v: m.power as number })),
              latest ? latest.t / 1000 : undefined,
              latest?.power,
            ),
            band: points
              .filter((m) => m.power_max != null)
              .map((m) => ({ t: m.ts, v: m.power_max as number })),
            max: Math.max(1, ...points.map((m) => m.power_max ?? 0)),
            fmt: fmtW,
            mark: tm.power
              ? {
                  v: tm.power.value,
                  label: `Tidemark ${fmtW(tm.power.value)} · ${fmtAt(tm.power.at)}`,
                }
              : null,
            peak: peakIn((m) => m.power_max ?? 0, fmtW),
            extra: () => null,
          },
        ]
      : []),
  ];

  // What crashed or stopped responding in the last fortnight (on the CPU tab).
  const crashesCard = (
    <section className="card chart" data-focus="crashes">
      <div className="row base wrap">
        <b className="cname">Crashes and hangs</b>
        <span className="muted small">last 14 days, from the reports macOS keeps</span>
      </div>
      <div className="crashlist">
        {crashGroups(crashList ?? []).length === 0 && (
          <span className="muted small">
            {crashList == null ? "Reading…" : "None. Nothing crashed or stopped responding."}
          </span>
        )}
        {crashGroups(crashList ?? [])
          .slice(0, 10)
          .map((g) => (
            <div className="rowline" key={`${g.app}${g.kind}`}>
              <span className="mono muted when">{fmtAt(g.last)}</span>
              <span className="col grow minw0">
                <b>{crashTitle(g)}</b>
                <span className="muted small">
                  {g.n > 1 ? `${g.n} reports, the last ${fmtAt(g.last)}` : "One report"}
                </span>
              </span>
            </div>
          ))}
      </div>
    </section>
  );

  // ---- the CPU and Memory tabs ----
  const cpuChart = charts.find((c) => c.name === "CPU");
  const memChart = charts.find((c) => c.name === "Memory");
  const from = now() - secs;
  const rangeIncidents = (incidents ?? []).filter((i) => (i.ended ?? now()) >= from);
  // A smaller chart of one part of the measure, on the page's time span.
  const part = (
    name: string,
    pts: Pt[],
    max: number,
    f: (v: number) => string,
    opts: { hi?: Pt[]; note?: string } = {},
  ) => (
    <div className="minw0" key={name}>
      <div className="row between small">
        <span className="eyebrow">{name}</span>
        <span className="muted">
          {opts.note ?? (pts.length ? `${f(pts[pts.length - 1].v)} now` : "")}
        </span>
      </div>
      {pts.length ? (
        <Area
          marker={at?.t}
          onPick={(p, atPeak) => pick(name, p.t, atPeak)}
          pts={pts}
          hi={opts.hi}
          max={max}
          height={110}
          domain={[from, now()]}
          sleeps={sleeps ?? []}
          xLabels={xLabels(secs, 3)}
          fmt={f}
          fmtT={(t) => fmtAt(t)}
          name={name}
          label={`${name} over the last ${RANGES[range].label}`}
        />
      ) : (
        <div className="empty small">Recording…</div>
      )}
    </div>
  );
  const series = (of: (m: Minute) => number | null, nowV?: number | null): Pt[] =>
    upToNow(
      points.map((m) => ({ t: m.ts, v: of(m) })).filter((p): p is Pt => p.v != null),
      latest ? latest.t / 1000 : undefined,
      nowV ?? undefined,
    );
  // The apps that used the most over the range, by one measure.
  const appsCard = (
    title: string,
    list: AppUse[],
    v: (a: AppUse) => number,
    f: (n: number) => string,
    other: (a: AppUse) => string,
  ) => (
    <section className="card list">
      <div className="row between head">
        <span className="eyebrow">
          {title} · {RANGES[range].label}
        </span>
      </div>
      {list.length === 0 && <div className="muted rowline">Nothing recorded in this range.</div>}
      <div className="blame">
        {list.slice(0, 6).map((a) => (
          <div className="brow" key={a.app}>
            <div className="row between">
              <span className="ell">{a.app}</span>
              <b>{f(v(a))}</b>
            </div>
            <span className="bar">
              <i style={{ width: `${v(list[0]) ? (100 * v(a)) / v(list[0]) : 0}%` }} />
            </span>
            <span className="muted small">{other(a)}</span>
          </div>
        ))}
      </div>
    </section>
  );
  // The range's incidents of some kinds; picking one shows its moment in the snapshot.
  const incidentsCard = (title: string, kinds: Incident["kind"][], none: string) => {
    const list = rangeIncidents.filter((i) => kinds.includes(i.kind));
    return (
      <section className="card list">
        <div className="row between head">
          <span className="eyebrow">
            {title} · {RANGES[range].label}
          </span>
        </div>
        {list.length === 0 && <div className="muted rowline">{none}</div>}
        <div className="col gap6">
          {list.slice(0, 6).map((i) => (
            <button
              className="incident"
              key={i.id}
              onClick={() => pickIncident(i)}
              title="Show what every app was doing at its peak"
            >
              <span>
                <b>{incidentTitle(i)}</b>{" "}
                <span className="muted">
                  {i.ended != null
                    ? `${Math.max(1, Math.round((i.ended - i.started) / 60))} min`
                    : "now"}
                </span>
              </span>
              <span className="muted">{fmtAt(i.started)}</span>
            </button>
          ))}
        </div>
      </section>
    );
  };
  // ---- the GPU, Disk I/O, Network and Energy tabs ----
  const gpuChart = charts.find((c) => c.name === "GPU");
  // A range's busiest moments by one measure; each opens its moment in the snapshot.
  const momentsCard = (
    title: string,
    name: string,
    by: (m: Minute) => number,
    f: (v: number) => string,
  ) => {
    const top = points
      .filter((m) => by(m) > 0)
      .sort((a, b) => by(b) - by(a))
      .slice(0, 5);
    return (
      <section className="card list">
        <div className="row between head">
          <span className="eyebrow">
            {title} · {RANGES[range].label}
          </span>
        </div>
        {top.length === 0 && <div className="muted rowline">Nothing recorded in this range.</div>}
        <div className="col gap6">
          {top.map((m) => (
            <button
              className="incident"
              key={m.ts}
              onClick={() => pick(name, m.ts, true)}
              title="Show what every app was doing then"
            >
              <b>{f(by(m))}</b>
              <span className="muted">{fmtAt(m.ts)}</span>
            </button>
          ))}
        </div>
      </section>
    );
  };
  // The Mac's thermal state now, and whether it ran hot in the range.
  const thermalTile = (
    <section className="card tile">
      <span className="eyebrow">
        <Labelled m="thermal">Thermal</Labelled>
      </span>
      <b className="tv">{latest ? (THERMAL[latest.thermal] ?? "Nominal") : "–"}</b>
      <span className="muted small">
        {points.some((m) => m.thermal >= 2)
          ? "Ran hot in this range"
          : "No throttling in this range"}
      </span>
    </section>
  );
  const powerTop = Math.max(1, ...points.map((m) => m.power_max ?? 0));
  const gpuTab = () => (
    <div className="perfmain" {...panel("gpu")}>
      {gpuChart && chartCard(gpuChart, 280)}
      <section className="card chart">
        <div className="row base wrap">
          <b className="cname">Alongside the GPU</b>
          <span className="muted small">
            macOS doesn't say which apps use the GPU, so these are what moves with it
          </span>
        </div>
        <div className="netpair">
          {part("CPU", ser("cpu"), 100, pct)}
          {points.some((m) => m.power != null) &&
            part(
              "Power",
              series((m) => m.power, latest?.power),
              powerTop,
              fmtW,
            )}
        </div>
      </section>
      <div className="cards2">
        {momentsCard("Busiest GPU moments", "GPU", (m) => m.gpu_max ?? 0, pct)}
        <div className="tiles">{thermalTile}</div>
      </div>
    </div>
  );
  const ioTab = () => (
    <div className="perfmain" {...panel("diskio")}>
      {ioCard(200)}
      <section className="card chart">
        <div className="row base wrap">
          <b className="cname">Swap</b>
          <span className="muted small">memory paged out to the disk: it writes as it grows</span>
        </div>
        <div className="netpair">
          {part("Swap", ser("swap"), Math.max(1e9, ...points.map((m) => m.swap_max)), fmt, {
            hi: ser("swap_max"),
          })}
        </div>
      </section>
      <div className="cards2">
        {momentsCard("Busiest disk moments", "Disk I/O", (m) => m.disk_max, fmtRate)}
        {appsCard(
          "Using the disk the most",
          rangeApps?.by_disk ?? [],
          (a) => a.disk ?? 0,
          fmtRate,
          (a) => `${pct(a.cpu)} CPU`,
        )}
      </div>
    </div>
  );
  const netTab = () => (
    <div className="perfmain" {...panel("network")}>
      {netCard(200)}
      <div className="cards2">
        {momentsCard("Busiest downloads", "Download", (m) => m.net_in_max || m.net_in, fmtRate)}
        {momentsCard("Busiest uploads", "Upload", (m) => m.net_out_max || m.net_out, fmtRate)}
      </div>
    </div>
  );
  const energyTab = () => (
    <div className="perfmain" {...panel("energy")}>
      {energyCard(240)}
      <section className="card chart">
        <div className="row base wrap">
          <b className="cname">What draws the power</b>
          <span className="muted small">the CPU and GPU, which most of the draw follows</span>
        </div>
        <div className="netpair">
          {part("CPU", ser("cpu"), 100, pct)}
          {gpuChart &&
            part(
              "GPU",
              series((m) => m.gpu, latest?.gpu),
              100,
              pct,
            )}
        </div>
      </section>
      <div className="cards2">
        {momentsCard("Most power drawn", "Energy", (m) => m.power_max ?? 0, fmtW)}
        <div className="tiles">{thermalTile}</div>
      </div>
    </div>
  );

  const nP = [...(live?.core_kinds ?? "")].filter((k) => k === "P").length;
  const nE = [...(live?.core_kinds ?? "")].filter((k) => k === "E").length;
  const cpuTab = () => (
    <div className="perfmain" {...panel("cpu")}>
      {cpuChart && chartCard(cpuChart, 280)}
      {cpuKinds && (
        <section className="card chart" data-focus="cores">
          <div className="row base wrap">
            <b className="cname">Performance and efficiency cores</b>
            <span className="muted small">
              {nP && nE ? `${nP} performance, ${nE} efficiency · ` : ""}each kind's average use
            </span>
          </div>
          <div className="netpair">
            {part("Performance cores", cpuKinds[0].pts, 100, pct)}
            {part("Efficiency cores", cpuKinds[1].pts, 100, pct)}
          </div>
        </section>
      )}
      <div className="cards2">
        {appsCard(
          "Using the most CPU",
          rangeApps?.by_cpu ?? [],
          (a) => a.cpu,
          pct,
          (a) => `${fmt(a.mem)} memory`,
        )}
        {incidentsCard(
          "CPU incidents",
          ["cpu"],
          "None: the CPU never stayed above 90% for 30 seconds.",
        )}
      </div>
      <div className="tiles">{thermalTile}</div>
      {crashesCard}
    </div>
  );
  const partsTop = Math.max(
    1,
    ...points.map((m) => Math.max(m.mem_app ?? 0, m.mem_wired ?? 0, m.mem_comp ?? 0)),
    latest?.mem_parts?.app ?? 0,
    latest?.mem_parts?.wired ?? 0,
    latest?.mem_parts?.compressed ?? 0,
  );
  const memTab = () => (
    <div className="perfmain" {...panel("memory")}>
      {memChart && chartCard(memChart, 280)}
      {points.some((m) => m.mem_app != null) && (
        <section className="card chart" data-focus="memory parts">
          <div className="row base wrap">
            <b className="cname">What the memory in use is made of</b>
            <span className="muted small">as Activity Monitor splits it, on one scale</span>
          </div>
          <div className="netpair">
            {part(
              "App memory",
              series((m) => m.mem_app, latest?.mem_parts?.app),
              partsTop,
              fmt,
            )}
            {part(
              "Wired",
              series((m) => m.mem_wired, latest?.mem_parts?.wired),
              partsTop,
              fmt,
            )}
            {part(
              "Compressed",
              series((m) => m.mem_comp, latest?.mem_parts?.compressed),
              partsTop,
              fmt,
            )}
          </div>
        </section>
      )}
      <section className="card chart" data-focus="swap">
        <div className="row base wrap">
          <b className="cname">Swap and pressure</b>
          <span className="muted small">
            swap grows when memory runs short; pressure is how macOS rates it
          </span>
        </div>
        <div className="netpair">
          {part("Swap", ser("swap"), Math.max(1e9, ...points.map((m) => m.swap_max)), fmt, {
            hi: ser("swap_max"),
          })}
          {part(
            "Pressure",
            series((m) => m.pressure, latest?.pressure),
            4,
            PRESSURE,
            {
              note: latest ? `${PRESSURE(latest.pressure)} now` : "",
            },
          )}
        </div>
      </section>
      <div className="cards2">
        {appsCard(
          "Using the most memory",
          rangeApps?.by_mem ?? [],
          (a) => a.mem,
          fmt,
          (a) => `${pct(a.cpu)} CPU`,
        )}
        {incidentsCard(
          "Memory incidents",
          ["pressure", "swap"],
          "None: memory never ran critically short and swap never grew by a gigabyte.",
        )}
      </div>
    </div>
  );

  // A remembered GPU tab on a Mac that reports no GPU shows everything.
  const shown: Tab = tab === "gpu" && !gpuChart ? "all" : tab;

  // A tab's content: its panel, named by its tab; `data-tab` lets a Details link find its card
  // on the tab being opened rather than on the one going away.
  const panel = (id: Tab) => ({
    role: "tabpanel",
    id: `ptab-${id}-panel`,
    "aria-labelledby": `ptab-${id}`,
    "data-tab": id,
  });
  const tabsShown = TABS.filter((t) => t.id !== "gpu" || gpuChart);
  const chooseTab = (id: Tab) => {
    setTab(id);
    if (picked?.mode !== "incident") setPicked(null);
  };

  const pickIncident = (i: Incident) => {
    const age = now() - i.peak_at;
    // Wide enough to show it, never narrower than the range already chosen.
    const fits = allowed.findIndex((x) => x.secs > age);
    const r = fits < 0 ? allowed.length - 1 : fits;
    if (r > range) setRange(r);
    setPicked({ t: i.peak_at, why: i.kind === "cpu" ? "CPU" : "Memory", mode: "incident", inc: i });
  };

  return (
    <>
      {bar(
        <>
          <span className="ttl">Recent</span>
          <div className="seg" role="group" aria-label="Time range">
            {allowed.map((r, i) => (
              <button
                key={r.label}
                className={i === range ? "on" : ""}
                aria-pressed={i === range}
                onClick={() => {
                  setRange(i);
                  setPicked(null);
                }}
                title={`CPU and memory over the last ${r.label}`}
              >
                {r.label}
              </button>
            ))}
          </div>
          <div className="grow" />
          <span className={`pill ${live?.paused_until ? "info" : "good"}`}>
            {live?.paused_until
              ? `Paused until ${fmtAt(live.paused_until)}`
              : "Recording · 1-minute detail"}
          </span>
        </>,
      )}
      <div className="page perf">
        <div className="ptabs" role="tablist" aria-label="Measures">
          {tabsShown.map((t, i) => (
            <button
              key={t.id}
              id={`ptab-${t.id}`}
              role="tab"
              aria-selected={shown === t.id}
              aria-controls={`ptab-${t.id}-panel`}
              tabIndex={shown === t.id ? 0 : -1}
              className={shown === t.id ? "on" : ""}
              onClick={() => chooseTab(t.id)}
              onKeyDown={(e) => {
                // Arrows, Home and End move between the tabs, as in any tab strip.
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
                document.getElementById(`ptab-${tabsShown[to].id}`)?.focus();
              }}
            >
              <span className="labelled">
                {t.m ? <MetricIcon m={t.m} /> : <AllIcon />}
                {t.label}
              </span>
            </button>
          ))}
        </div>
        {shown === "cpu" ? (
          cpuTab()
        ) : shown === "gpu" ? (
          gpuTab()
        ) : shown === "memory" ? (
          memTab()
        ) : shown === "diskio" ? (
          ioTab()
        ) : shown === "network" ? (
          netTab()
        ) : shown === "energy" ? (
          energyTab()
        ) : (
          <div className="perfmain" {...panel("all")}>
            {charts.map((c) => chartCard(c, 170, false))}
            {allMore().map((c) => chartCard(c, 170, false))}
            <section className="card chart" data-focus="disk space">
              <div className="row base wrap">
                <b className="cname">
                  <Labelled m="disk">Disk space</Labelled>
                </b>
                {(() => {
                  const pts = diskRange?.points ?? [];
                  const last = pts[pts.length - 1];
                  const first = pts[0];
                  return (
                    <>
                      <span
                        className={`cnow${flagFor(live?.status, "Disk space") ? ` ${flagFor(live?.status, "Disk space")?.cls}` : ""}`}
                      >
                        {last ? `${fmt(last.total - last.used)} free` : "–"}
                      </span>
                      {flagFor(live?.status, "Disk space") && (
                        <span className={`badge ${flagFor(live?.status, "Disk space")?.tone}`}>
                          {flagFor(live?.status, "Disk space")?.label}
                        </span>
                      )}
                      {flagFor(live?.status, "Disk space") && last && (
                        <AskButton
                          title="Ask what is filling the disk (opens Ask with the disk's figures)"
                          onAsk={() => {
                            const { question, detail } = flaggedDetail(live, "Disk space", [
                              `${fmt(last.total - last.used)} free: ${fmt(last.used)} of ${fmt(last.total)} used.`,
                              first && pts.length > 1
                                ? `${fmtDelta(last.used - first.used)} over the last ${secs < 86400 ? "24 hours" : RANGES[range].label}.`
                                : "",
                            ]);
                            ask(question, detail);
                          }}
                        />
                      )}
                      <span className="muted small">
                        {last ? `${fmt(last.used)} of ${fmt(last.total)} used` : ""}
                        {first && last && pts.length > 1
                          ? ` · ${fmtDelta(last.used - first.used)} over the last ${secs < 86400 ? "24 hours" : RANGES[range].label}`
                          : ""}
                      </span>
                    </>
                  );
                })()}
                <span className="grow" />
                <Legend line="Disk space used, hourly" mark="Week's tidemark" />
              </div>
              {(diskRange?.points.length ?? 0) > 1 ? (
                (() => {
                  const pts = (diskRange?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
                  const lo = Math.min(...pts.map((p) => p.v));
                  const hi = Math.max(...pts.map((p) => p.v), diskWeek?.high?.used ?? 0);
                  const pad = Math.max((hi - lo) * 0.15, 1e9);
                  return (
                    <Area
                      marker={at?.t}
                      pts={pts}
                      max={hi + pad}
                      min={Math.max(0, lo - pad)}
                      height={170}
                      mark={
                        diskWeek?.high
                          ? {
                              v: diskWeek.high.used,
                              label: `Tidemark ${fmt(diskWeek.high.used)} · ${fmtAt(diskWeek.high.ts, false)}`,
                            }
                          : null
                      }
                      fmt={fmt}
                      fmtT={(t) => fmtAt(t)}
                      name="Disk space used"
                      domain={[now() - diskSecs, now()]}
                      xLabels={xLabels(diskSecs, 4)}
                      label={`Disk space used over the last ${secs < 86400 ? "24 hours" : RANGES[range].label}`}
                    />
                  );
                })()
              ) : (
                <div className="empty small">The disk's used space is recorded every hour.</div>
              )}
            </section>
          </div>
        )}

        <aside className="card snap">
          <div>
            <div className="row between">
              <span className="eyebrow">
                {at ? measure.what.charAt(0).toUpperCase() + measure.what.slice(1) : "Snapshot"}
              </span>
            </div>
            {at && (
              <span className="muted small">
                {(() => {
                  const when = fmtAt(at.t);
                  return when.charAt(0).toUpperCase() + when.slice(1);
                })()}
                {!picked ? " · the range's peak" : ""}
              </span>
            )}
            {atAvg != null && (
              <div className="snapstats">
                <div>
                  <b>{measure.fmt(atAvg)}</b>
                  <span className="muted small">{measure.avgLabel ?? `${per} average`}</span>
                </div>
                {atPeak != null && (
                  <div>
                    <b>{measure.fmt(atPeak)}</b>
                    <span className="muted small">peak</span>
                  </div>
                )}
              </div>
            )}
          </div>
          {measure.note && <div className="muted small">{measure.note}</div>}
          {at && (atPoint || mode === "incident") && <span className="eyebrow">{listLabel}</span>}
          <div className="blame">
            {blame.length === 0 && (
              <div className="muted small">Nothing recorded at that moment.</div>
            )}
            {blame.slice(0, 6).map((p) => (
              <div className="brow" key={p.app}>
                <div className="row between">
                  <span className="ell">{p.app}</span>
                  <b>{appF(appV(p))}</b>
                </div>
                <span className="bar">
                  <i
                    style={{ width: `${appV(blame[0]) ? (100 * appV(p)) / appV(blame[0]) : 0}%` }}
                  />
                </span>
                <span className="muted small">
                  {measure.by === "mem" ? `${pct(p.cpu)} CPU` : `${fmt(p.mem)} memory`}
                </span>
              </div>
            ))}
          </div>
          {measure === MEASURES.CPU && atPeak != null && (
            <div className="note">
              {atPeak >= 90
                ? "It went above 90%, so if it lasted 30 seconds it was recorded as an incident."
                : "It stayed under 90%, so it isn't an incident."}
            </div>
          )}
          <div className="col gap6">
            <span className="eyebrow">Incidents · last 14 days</span>
            {(incidents ?? []).length === 0 && (
              <span className="muted small">None. Calm fortnight.</span>
            )}
            {(incidents ?? []).slice(0, 6).map((i) => (
              <button
                className="incident"
                key={i.id}
                onClick={() => pickIncident(i)}
                title="Show what every app was doing at its peak"
              >
                <span>
                  <b>{incidentTitle(i)}</b>{" "}
                  <span className="muted">
                    {i.ended != null
                      ? `${Math.max(1, Math.round((i.ended - i.started) / 60))} min`
                      : "now"}
                  </span>
                </span>
                <span className="muted">{fmtAt(i.started)}</span>
              </button>
            ))}
            <span className="muted small">
              Each one keeps a snapshot of what every app was doing.
            </span>
          </div>
          <div className="row wrap gap8">
            <button
              className="btn primary"
              disabled={!assistant.available}
              title={
                assistant.available
                  ? "Put this moment to Ask (⌘K)"
                  : (assistant.reason ?? undefined)
              }
              onClick={() =>
                ask(
                  at
                    ? `Why was the ${measure.what} ${atAvg != null ? `at ${measure.fmt(atAvg)}` : "high"} at ${fmtAt(at.t)}${top ? `, with ${top.app} on top` : ""}?`
                    : "Is anything slowing my Mac down?",
                )
              }
            >
              Ask why this happened
            </button>
            <button
              className="btn"
              onClick={() => void api.activityMonitor()}
              title="Open macOS's Activity Monitor for every process, live"
            >
              Activity Monitor
            </button>
          </div>
        </aside>
      </div>
    </>
  );
}
