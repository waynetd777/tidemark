// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Now: how the Mac is doing, in a sentence; CPU, memory and disk with their tidemarks; the
// week's high-water marks and incidents; what grew on the disk; and questions for Ask.

import { useAssistant } from "./assistant";
import type { ReactNode } from "react";
import type { AskDetail } from "./Ask";
import { AskButton } from "./AskButton";
import {
  crashDetail,
  crashQuestion,
  flaggedDetail,
  incidentDetail,
  incidentQuestion,
} from "./askDetails";
import { Labelled } from "./icons";
import { metricOf } from "./metrics";
import { useRemembered } from "./remember";
import { flagFor } from "./status";
import { api } from "./api";
import { Area, Cores, Legend, Meter, type Pt } from "./charts";
import { crashGroups, crashTitle, incidentTitle, incidentWhy } from "./incidents";
import { gapsOf, now, startOfToday, upToNow, usePoll } from "./perf";
import type { AppUse, Crash, GrowthInfo, Incident, Live, Screen } from "./types";
import { abbrev, batteryLine, fmt, fmtAt, fmtDelta, fmtMem, fmtRate, fmtW, pct } from "./util";

const DAY = 86400;
const HOUR = 3600;

export default function Now({
  live,
  growth,
  home,
  bar,
  go,
  ask,
}: {
  live: Live | null;
  growth: GrowthInfo | null;
  home: string;
  bar: (left: ReactNode) => ReactNode;
  go: (s: Screen, target?: string) => void;
  ask: (q: string, detail?: AskDetail) => void;
}) {
  const assistant = useAssistant();
  // The CPU card's cores, shown when asked for (the chevron), and remembered.
  const [coresOpen, setCoresOpen] = useRemembered(
    "now.cores",
    false,
    (v): v is boolean => typeof v === "boolean",
  );
  // The charts: the last hour, minute by minute. The day: today's highs and totals.
  const hour = usePoll(() => api.perf(HOUR, 60), 30_000);
  const day = usePoll(() => api.perf(DAY, 288), 60_000);
  // The disk card's line is the last day; its tidemark and the week's list use the week.
  const disk = usePoll(() => api.diskTrend(7), 300_000);
  const diskDay = usePoll(() => api.diskTrend(1), 300_000);
  const incidents = usePoll(() => api.incidents(7), 60_000);
  const crashes = usePoll(() => api.crashes(7), 300_000);
  const sleeps = usePoll(() => api.sleeps(1), 60_000);
  const latest = live?.samples[live.samples.length - 1];
  const today = startOfToday();
  const nowS = now();
  const total = live?.mem_total || day?.points[0]?.mem_total || 1;

  let upHigh: { v: number; t: number } | null = null;
  for (const m of day?.points ?? [])
    if (m.ts >= today && (!upHigh || m.net_out_max > upHigh.v))
      upHigh = { v: m.net_out_max, t: m.ts };
  let ioHigh: { v: number; t: number } | null = null;
  for (const m of day?.points ?? [])
    if (m.ts >= today && (!ioHigh || m.disk_max > ioHigh.v)) ioHigh = { v: m.disk_max, t: m.ts };
  const series = (k: "cpu" | "mem" | "net_in"): Pt[] =>
    upToNow(
      (hour?.points ?? []).map((m) => ({ t: m.ts, v: m[k] })),
      latest ? latest.t / 1000 : undefined,
      latest?.[k],
    );
  const highToday = (k: "cpu_max" | "mem_max" | "gpu_max") => {
    let best: { v: number; t: number } | null = null;
    for (const m of day?.points ?? []) {
      const v = m[k];
      if (m.ts >= today && v != null && (!best || v > best.v)) best = { v, t: m.ts };
    }
    return best;
  };
  const gpuHigh = highToday("gpu_max");
  const hasGpu = (hour?.points ?? []).some((m) => m.gpu != null) || latest?.gpu != null;
  const movedToday = (k: "net_in" | "net_out") =>
    (day?.points ?? [])
      .filter((m) => m.ts >= today)
      .reduce((sum, m) => sum + m[k] * (m.minutes || 1) * 60, 0);
  let netHigh: { v: number; t: number } | null = null;
  for (const m of day?.points ?? [])
    if (m.ts >= today && (!netHigh || m.net_in_max > netHigh.v))
      netHigh = { v: m.net_in_max, t: m.ts };
  const cpuHigh = highToday("cpu_max");
  const memHigh = highToday("mem_max");
  const tm = hour?.tidemarks ?? day?.tidemarks ?? {};
  const diskPts: Pt[] = (disk?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const dayDiskPts: Pt[] = (diskDay?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const diskTotal = disk?.points[disk.points.length - 1]?.total ?? 0;
  const diskNow = diskPts[diskPts.length - 1]?.v ?? 0;
  const busy = (live?.apps_cpu ?? []).filter((a) => a.cpu >= 5).length;
  const landed = (growth?.landed ?? []).filter((g) => g.delta > 0);
  const grewTotal = Math.max(0, growth?.total ?? 0);
  const st = live?.status;

  const marks: {
    t: number;
    what: string;
    why: string;
    incident: boolean;
    crash?: "crash" | "hang";
    /// What its Ask button asks about.
    inc?: Incident;
    group?: { app: string; kind: Crash["kind"]; n: number; last: number };
  }[] = [
    ...(incidents ?? []).map((i) => ({
      t: i.started,
      what: incidentTitle(i),
      why: incidentWhy(i),
      incident: true,
      inc: i,
    })),
    // Crashes and hangs this week, one line per app.
    ...crashGroups(crashes ?? []).map((g) => ({
      t: g.last,
      what: crashTitle(g),
      why: g.n > 1 ? `${g.n} reports this week.` : "From the report macOS wrote.",
      incident: false,
      crash: g.kind,
      group: g,
    })),
  ];
  if (disk?.high && diskPts.length > 1)
    marks.push({
      t: disk.high.ts,
      what: `Disk reached ${fmt(disk.high.used)} used`,
      why:
        diskNow < disk.high.used - 1e9
          ? `${fmt(disk.high.used - diskNow)} has been freed since.`
          : "The highest it has been this week.",
      incident: false,
    });
  marks.sort((a, b) => a.t - b.t);

  const sub =
    (st?.sub ?? "") +
    (grewTotal > 1e9 && landed[0]
      ? ` The disk grew ${fmt(grewTotal)} this week, most of it in ${abbrev(landed[0].path, home)}.`
      : "");

  const cards = [
    {
      name: "CPU",
      line: "CPU, minute average",
      range: "Last hour",
      value: latest ? Math.round(latest.cpu).toString() : "–",
      unit: "%",
      sub: `${live?.cores ?? "–"} cores · ${busy} app${busy === 1 ? "" : "s"} busy`,
      pts: series("cpu"),
      level: latest ? latest.cpu / 100 : null,
      max: 100,
      mark: tm.cpu
        ? { v: tm.cpu.value, label: `Tidemark ${pct(tm.cpu.value)} · ${fmtAt(tm.cpu.at)}` }
        : null,
      high: cpuHigh
        ? `Today's high ${pct(cpuHigh.v)} at ${fmtAt(cpuHigh.t)}`
        : "No reading today yet",
      fmt: pct,
      to: "performance" as Screen,
    },
    // The GPU, where the Mac reports how busy it is.
    ...(hasGpu
      ? [
          {
            name: "GPU",
            line: "GPU, minute average",
            range: "Last hour",
            value: latest?.gpu != null ? Math.round(latest.gpu).toString() : "–",
            unit: "%",
            sub: "Graphics, video and machine learning",
            pts: upToNow(
              (hour?.points ?? [])
                .filter((m) => m.gpu != null)
                .map((m) => ({ t: m.ts, v: m.gpu as number })),
              latest ? latest.t / 1000 : undefined,
              latest?.gpu,
            ),
            level: latest?.gpu != null ? latest.gpu / 100 : null,
            max: 100,
            mark: tm.gpu
              ? { v: tm.gpu.value, label: `Tidemark ${pct(tm.gpu.value)} · ${fmtAt(tm.gpu.at)}` }
              : null,
            high: gpuHigh
              ? `Today's high ${pct(gpuHigh.v)} at ${fmtAt(gpuHigh.t)}`
              : "No reading today yet",
            fmt: pct,
            to: "performance" as Screen,
          },
        ]
      : []),
    {
      name: "Memory",
      line: "Memory used, minute average",
      range: "Last hour",
      value: latest ? (latest.mem / 2 ** 30).toFixed(1) : "–",
      unit: total > 1 ? `of ${fmtMem(total)}` : "",
      sub: latest
        ? `Pressure ${latest.pressure >= 4 ? "critical" : latest.pressure >= 2 ? "raised" : "normal"} · swap ${fmtMem(latest.swap)}`
        : "",
      pts: series("mem"),
      level: latest && total > 1 ? latest.mem / total : null,
      max: total,
      mark: tm.mem
        ? { v: tm.mem.value, label: `Tidemark ${fmtMem(tm.mem.value)} · ${fmtAt(tm.mem.at)}` }
        : null,
      high: memHigh
        ? `Today's high ${fmtMem(memHigh.v)} at ${fmtAt(memHigh.t)}`
        : "No reading today yet",
      fmt: fmtMem,
      to: "performance" as Screen,
    },
    {
      name: "Disk I/O",
      line: "Reads and writes, minute average",
      range: "Last hour",
      value: latest ? fmtRate(latest.disk_r + latest.disk_w).split(" ")[0] : "–",
      unit: latest ? fmtRate(latest.disk_r + latest.disk_w).split(" ")[1] : "",
      sub: latest ? `Reading ${fmtRate(latest.disk_r)} · writing ${fmtRate(latest.disk_w)}` : "",
      pts: upToNow(
        (hour?.points ?? []).map((m) => ({ t: m.ts, v: m.disk_r + m.disk_w })),
        latest ? latest.t / 1000 : undefined,
        latest ? latest.disk_r + latest.disk_w : undefined,
      ),
      // Rates fill against the hour's busiest moment, so a quiet minute reads as quiet.
      level: latest
        ? (latest.disk_r + latest.disk_w) /
          Math.max(1, ...(hour?.points ?? []).map((m) => Math.max(m.disk_max, m.disk_r + m.disk_w)))
        : null,
      max: Math.max(1, ...(hour?.points ?? []).map((m) => m.disk_r + m.disk_w)),
      mark: null,
      high: ioHigh ? `Busiest ${fmtRate(ioHigh.v)} at ${fmtAt(ioHigh.t)}` : "Quiet so far today",
      fmt: fmtRate,
      to: "performance" as Screen,
    },
    {
      name: "Network download",
      focus: "network",
      line: "Download, minute average",
      range: "Last hour",
      value: latest ? fmtRate(latest.net_in).split(" ")[0] : "–",
      unit: latest ? `${fmtRate(latest.net_in).split(" ")[1]} ↓` : "",
      sub: latest ? `${fmt(movedToday("net_in"))} down today` : "",
      pts: series("net_in"),
      level: latest
        ? latest.net_in /
          Math.max(1, ...(hour?.points ?? []).map((m) => Math.max(m.net_in_max, m.net_in)))
        : null,
      max: Math.max(1, ...(hour?.points ?? []).map((m) => m.net_in)),
      mark: null,
      high: netHigh ? `Busiest ${fmtRate(netHigh.v)} at ${fmtAt(netHigh.t)}` : "Quiet so far today",
      fmt: fmtRate,
      to: "performance" as Screen,
    },
    {
      name: "Network upload",
      focus: "network",
      line: "Upload, minute average",
      range: "Last hour",
      value: latest ? fmtRate(latest.net_out).split(" ")[0] : "–",
      unit: latest ? `${fmtRate(latest.net_out).split(" ")[1]} ↑` : "",
      sub: latest ? `${fmt(movedToday("net_out"))} up today` : "",
      pts: upToNow(
        (hour?.points ?? []).map((m) => ({ t: m.ts, v: m.net_out })),
        latest ? latest.t / 1000 : undefined,
        latest?.net_out,
      ),
      level: latest
        ? latest.net_out /
          Math.max(1, ...(hour?.points ?? []).map((m) => Math.max(m.net_out_max, m.net_out)))
        : null,
      max: Math.max(1, ...(hour?.points ?? []).map((m) => m.net_out)),
      mark: null,
      high: upHigh ? `Busiest ${fmtRate(upHigh.v)} at ${fmtAt(upHigh.t)}` : "Quiet so far today",
      fmt: fmtRate,
      to: "performance" as Screen,
    },
    {
      name: "Energy",
      line: "Power drawn, minute average",
      range: "Last hour",
      value: latest?.power != null ? fmtW(latest.power).split(" ")[0] : "–",
      unit: latest?.power != null ? "W" : "",
      sub: live?.battery
        ? `Battery ${batteryLine(live.battery)}`
        : live
          ? "No battery on this Mac"
          : "",
      pts: upToNow(
        (hour?.points ?? [])
          .filter((m) => m.power != null)
          .map((m) => ({ t: m.ts, v: m.power as number })),
        latest ? latest.t / 1000 : undefined,
        latest?.power,
      ),
      level:
        latest?.power != null
          ? latest.power /
            Math.max(
              1,
              tm.power?.value ?? 0,
              ...(hour?.points ?? []).map((m) => m.power_max ?? m.power ?? 0),
            )
          : null,
      max: Math.max(1, ...(hour?.points ?? []).map((m) => m.power ?? 0)),
      mark: tm.power
        ? { v: tm.power.value, label: `Tidemark ${fmtW(tm.power.value)} · ${fmtAt(tm.power.at)}` }
        : null,
      high: live?.apps_energy[0]?.energy
        ? `Using the most: ${live.apps_energy[0].app}`
        : "Energy appears after a few readings",
      fmt: fmtW,
      to: "performance" as Screen,
    },
    {
      name: "Disk space",
      line: "Disk space used, hourly",
      range: "Last 24 h",
      value: diskTotal ? Math.round((diskTotal - diskNow) / 1e9).toString() : "–",
      unit: "GB free",
      sub: diskTotal ? `${fmt(diskNow)} of ${fmt(diskTotal)} used` : "Recorded hourly",
      pts: dayDiskPts,
      level: diskTotal ? diskNow / diskTotal : null,
      max: Math.max(1, ...dayDiskPts.map((p) => p.v), disk?.high?.used ?? 0) * 1.005,
      min: dayDiskPts.length ? Math.min(...dayDiskPts.map((p) => p.v)) * 0.995 : 0,
      mark: disk?.high
        ? {
            v: disk.high.used,
            label: `Tidemark ${fmt(disk.high.used)} · ${fmtAt(disk.high.ts, false)}`,
          }
        : null,
      high:
        disk?.high && diskNow < disk.high.used - 1e9
          ? `Freed ${fmt(disk.high.used - diskNow)} since ${fmtAt(disk.high.ts, false)}`
          : "At its high-water mark",
      fmt,
      to: "performance" as Screen,
    },
  ];

  const questions = [
    marks.find((m) => m.incident)
      ? `Why was my Mac slow ${(() => {
          const d = fmtAt(marks.filter((m) => m.incident).slice(-1)[0].t, false);
          return d === "today" || d === "yesterday" ? d : `on ${d}`;
        })()}?`
      : "Is anything slowing my Mac down?",
    landed[0]
      ? `What can I clear in ${abbrev(landed[0].path, home)}?`
      : "Where can I free space safely?",
    "Is the disk filling faster than usual?",
  ];

  const perCore = (live?.core_use.length ?? 0) > 1;
  // The week's list in two: the peaks (incidents, the disk's high), and what crashed or hung.
  const peaks = marks.filter((m) => !m.crash);
  const breaks = marks.filter((m) => m.crash);
  const markRow = (m: (typeof marks)[number], i: number) => (
    <div className="rowline" key={i}>
      <span className="mono muted when">{fmtAt(m.t)}</span>
      <span className="col">
        <span className="row wrap">
          <b>{m.what}</b>
          {m.incident && <span className="badge warn">Incident · snapshot kept</span>}
          {m.crash && (
            <span className="badge warn">
              {m.crash === "hang" ? "Hang report" : "Crash report"}
            </span>
          )}
        </span>
        <span className="muted small">{m.why}</span>
      </span>
      {(m.inc || m.group) && (
        <AskButton
          title={
            m.inc
              ? "Ask why this happened, with what the 12 busiest apps were using at its peak"
              : "Ask what went wrong, with the reports macOS wrote"
          }
          onAsk={async () => {
            if (m.inc) ask(incidentQuestion(m.inc), incidentDetail(m.inc));
            else if (m.group)
              ask(crashQuestion(m.group), await crashDetail(m.group, crashes ?? []));
          }}
        />
      )}
    </div>
  );

  // One measure's card: its figure, a chart of the range with the tidemark, and its high.
  const card = (c: (typeof cards)[number]) => {
    const flag = flagFor(st, c.name);
    return (
      <section className="card metric" key={c.name}>
        <div className="row between">
          <span className="eyebrow">
            <Labelled m={metricOf(c.name)}>{c.name}</Labelled>
          </span>
          {flag ? (
            <span className="row gap6">
              <span className={`badge ${flag.tone}`}>{flag.label}</span>
              <AskButton
                title={`Ask why ${c.name.toLowerCase()} is flagged (opens Ask with this chart's figures)`}
                onAsk={() => {
                  const pts = c.pts.filter((p) => p.v != null);
                  const vals = pts.map((p) => p.v);
                  const f = c.fmt;
                  const { question, detail } = flaggedDetail(live, c.name, [
                    `Now: ${c.value} ${c.unit}. ${c.sub}.`,
                    vals.length
                      ? `${c.range}: lowest ${f(Math.min(...vals))}, highest ${f(Math.max(...vals))}, average ${f(vals.reduce((a, b) => a + b, 0) / vals.length)}.`
                      : "",
                    c.mark ? `The week's tidemark: ${c.mark.label}.` : "",
                    c.high ? `${c.high}.` : "",
                  ]);
                  ask(question, detail);
                }}
              />
            </span>
          ) : (
            <span className="muted small">{c.range}</span>
          )}
        </div>
        <div className={`big${flag ? ` ${flag.cls}` : ""}`}>
          <span className="num">{c.value}</span>
          <span className={`unit${flag ? "" : " muted"}`}>{c.unit}</span>
        </div>
        {perCore && c.name === "CPU" ? (
          <div className="row between">
            <span className="muted small">{c.sub}</span>
            <button
              className={`chev${coresOpen ? " open" : ""}`}
              onClick={() => setCoresOpen(!coresOpen)}
              aria-expanded={coresOpen}
              title={coresOpen ? "Hide each core" : "Show each core"}
            >
              <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                <path d="M3 4.5l3 3 3-3" />
              </svg>
            </button>
          </div>
        ) : (
          <div className="muted small">{c.sub}</div>
        )}
        <Meter level={c.level} tone={flag?.tone} label={`${c.name} now`} />
        {perCore && coresOpen && c.name === "CPU" && (
          <Cores use={live!.core_use} kinds={live!.core_kinds} tone={flag?.tone} />
        )}
        <Area
          pts={c.pts}
          max={c.max}
          min={c.min ?? 0}
          // The CPU card's chart gives up the room its cores take, so the cards stay level.
          height={perCore && coresOpen && c.name === "CPU" ? 112 : 150}
          axis={false}
          domain={[nowS - (c.range === "Last 24 h" ? DAY : HOUR), nowS]}
          sleeps={c.name === "Disk space" ? undefined : (sleeps ?? [])}
          tone={flag?.tone}
          mark={c.mark}
          fmt={c.fmt}
          fmtT={(t) => fmtAt(t)}
          name={c.name}
          label={`${c.name}, ${c.range.toLowerCase()}`}
        />
        <Legend
          line={c.line}
          mark={c.mark ? "Week's tidemark" : undefined}
          gaps={
            c.name === "Disk space"
              ? undefined
              : gapsOf(
                  c.pts.map((p) => p.t),
                  [nowS - HOUR, nowS],
                  sleeps ?? [],
                )
          }
        />
        <div className="row between small mfoot">
          <span className="muted ell" title={c.high}>
            {c.high}
          </span>
          <button
            className="link"
            onClick={() =>
              go(
                c.to,
                c.to === "disk"
                  ? undefined
                  : (("focus" in c ? c.focus : undefined) ?? c.name.toLowerCase()),
              )
            }
            title={`See ${c.name.toLowerCase()} in detail`}
          >
            Details
          </button>
        </div>
      </section>
    );
  };
  return (
    <>
      {bar(
        <>
          <span className="ttl">Now</span>
          <span className="muted small">
            {new Date().toLocaleDateString("en-GB", {
              weekday: "long",
              day: "numeric",
              month: "long",
            })}
          </span>
        </>,
      )}
      <div className="page">
        <div className="hero">
          <h1>
            {st?.headline ?? "Starting to record"}
            <span
              className={
                st?.severity === "critical" ? "badt" : st?.level === "attention" ? "warnt" : "tidet"
              }
            >
              .
            </span>
          </h1>
          <p className="muted">{sub || "The first readings arrive in a few seconds."}</p>
        </div>

        <div className="cards4">{cards.map((c) => card(c))}</div>

        <div className="cards2 halves" data-focus="apps-now">
          {appsCard(
            "Memory",
            live,
            live?.apps_mem,
            (a) => a.mem,
            fmtMem,
            (a) => `${pct(a.cpu)} CPU`,
          )}
          {appsCard(
            "CPU",
            live,
            live?.apps_cpu,
            (a) => a.cpu,
            pct,
            (a) => fmtMem(a.mem),
          )}
          {appsCard(
            "Disk I/O",
            live,
            live?.apps_disk,
            (a) => a.disk ?? 0,
            fmtRate,
            (a) => `${pct(a.cpu)} CPU`,
          )}
          {appsCard(
            "Energy",
            live,
            live?.apps_energy,
            (a) => a.energy,
            fmtW,
            (a) => `${pct(a.cpu)} CPU`,
          )}
        </div>

        <div className="cards2 thirds">
          <section className="card list">
            <div className="row between head">
              <span className="eyebrow">This week's tidemarks</span>
              <button className="link small" onClick={() => go("trends")} title="Open Trends (⌘3)">
                All trends
              </button>
            </div>
            {peaks.length === 0 && (
              <div className="muted rowline">
                No incidents this week. Peaks appear here as they happen.
              </div>
            )}
            {peaks.slice(-5).map(markRow)}
          </section>
          <section className="card list">
            <div className="row between head">
              <span className="eyebrow">Crashes and hangs this week</span>
            </div>
            {breaks.length === 0 && (
              <div className="muted rowline">No app crashed or hung this week.</div>
            )}
            {breaks.slice(-5).map(markRow)}
          </section>
          <section className="card list">
            <div className="row between head">
              <span className="eyebrow">What grew on disk this week</span>
              {grewTotal > 0 && <span className="muted small">{fmtDelta(grewTotal)} in all</span>}
            </div>
            {landed.length === 0 && (
              <div className="muted rowline">
                {growth?.from
                  ? "Nothing grew by more than 100 MB."
                  : "Growth shows once two scans are 20 hours or more apart."}
              </div>
            )}
            {landed.slice(0, 5).map((g) => (
              <div className="grewrow" key={g.path}>
                <span className="mono ell" title={g.path}>
                  {abbrev(g.path, home)}
                </span>
                <span className="bar">
                  <i style={{ width: `${(100 * g.delta) / landed[0].delta}%` }} />
                </span>
                <b className="r">{fmtDelta(g.delta)}</b>
              </div>
            ))}
            <div className="grow" />
            <button
              className="link small left"
              onClick={() => go("disk")}
              title="Open the map with this week's growth marked (⌘4)"
            >
              Show the growth on the map →
            </button>
          </section>
        </div>

        <section className="askstrip" title={assistant.reason ?? undefined}>
          <span className="lbl">Ask Tidemark</span>
          {questions.map((q) => (
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
    </>
  );
}

/// The apps using the most of one measure, live: five deep, with another measure beside each
/// name. Apps using none of it (an idle disk) are left out.
function appsCard(
  measure: "Memory" | "CPU" | "Disk I/O" | "Energy",
  live: Live | null,
  apps: AppUse[] | undefined,
  value: (a: AppUse) => number,
  show: (n: number) => string,
  other: (a: AppUse) => string,
) {
  const top = (apps ?? []).filter((a) => value(a) > 0).slice(0, 5);
  const most = top.length ? value(top[0]) : 0;
  return (
    <section className="card list">
      <div className="row between head">
        <span className="eyebrow">
          <Labelled m={metricOf(measure)}>
            Using the most {measure === "CPU" ? measure : measure.toLowerCase()} right now
          </Labelled>
        </span>
      </div>
      {top.length === 0 && (
        <div className="muted rowline">
          {live ? "No app is using any right now." : "The first readings arrive in a few seconds."}
        </div>
      )}
      {top.map((a) => (
        <div className="grewrow" key={a.app}>
          <span className="ell">
            {a.app} <span className="tiny muted">{other(a)}</span>
          </span>
          <span className="bar">
            <i style={{ width: `${most > 0 ? (100 * value(a)) / most : 0}%` }} />
          </span>
          <b className="r">{show(value(a))}</b>
        </div>
      ))}
    </section>
  );
}
