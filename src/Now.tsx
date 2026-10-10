// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// Now: how the Mac is doing, in a sentence; CPU, memory and disk with their tidemarks; the
// week's high-water marks and incidents; what grew on the disk; and questions for Ask.

import type { ReactNode } from "react";
import { api } from "./api";
import { Area, type Pt } from "./charts";
import { incidentTitle, incidentWhy } from "./incidents";
import { startOfToday, usePoll } from "./perf";
import type { GrowthInfo, Live, Screen } from "./types";
import { abbrev, fmt, fmtAt, fmtDelta, fmtRate, pct } from "./util";

const DAY = 86400;

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
  go: (s: Screen) => void;
  ask: (q: string) => void;
}) {
  const day = usePoll(() => api.perf(DAY, 288), 60_000);
  const disk = usePoll(() => api.diskTrend(7), 300_000);
  const incidents = usePoll(() => api.incidents(7), 60_000);
  const latest = live?.samples[live.samples.length - 1];
  const today = startOfToday();
  const total = live?.mem_total || day?.points[0]?.mem_total || 1;

  const series = (k: "cpu" | "mem" | "net_in"): Pt[] =>
    (day?.points ?? []).map((m) => ({ t: m.ts, v: m[k] }));
  const highToday = (k: "cpu_max" | "mem_max") => {
    let best: { v: number; t: number } | null = null;
    for (const m of day?.points ?? [])
      if (m.ts >= today && (!best || m[k] > best.v)) best = { v: m[k], t: m.ts };
    return best;
  };
  const movedToday = (k: "net_in" | "net_out") =>
    (day?.points ?? [])
      .filter((m) => m.ts >= today)
      .reduce((sum, m) => sum + m[k] * (m.minutes || 1) * 60, 0);
  let netHigh: { v: number; t: number } | null = null;
  for (const m of day?.points ?? [])
    if (m.ts >= today && (!netHigh || m.net_in > netHigh.v)) netHigh = { v: m.net_in, t: m.ts };
  const cpuHigh = highToday("cpu_max");
  const memHigh = highToday("mem_max");
  const tm = day?.tidemarks ?? {};
  const diskPts: Pt[] = (disk?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const diskTotal = disk?.points[disk.points.length - 1]?.total ?? 0;
  const diskNow = diskPts[diskPts.length - 1]?.v ?? 0;
  const busy = (live?.apps_cpu ?? []).filter((a) => a.cpu >= 5).length;
  const landed = (growth?.landed ?? []).filter((g) => g.delta > 0);
  const grewTotal = Math.max(0, growth?.total ?? 0);
  const st = live?.status;

  const marks: { t: number; what: string; why: string; incident: boolean }[] = [
    ...(incidents ?? []).map((i) => ({
      t: i.started,
      what: incidentTitle(i),
      why: incidentWhy(i),
      incident: true,
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
      range: "Last 24 h",
      value: latest ? Math.round(latest.cpu).toString() : "–",
      unit: "%",
      sub: `${live?.cores ?? "–"} cores · ${busy} app${busy === 1 ? "" : "s"} busy`,
      pts: series("cpu"),
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
    {
      name: "Memory",
      range: "Last 24 h",
      value: latest ? (latest.mem / 1e9).toFixed(1) : "–",
      unit: `of ${fmt(total)}`,
      sub: latest
        ? `Pressure ${latest.pressure >= 4 ? "critical" : latest.pressure >= 2 ? "raised" : "normal"} · swap ${fmt(latest.swap)}`
        : "",
      pts: series("mem"),
      max: total,
      mark: tm.mem
        ? { v: tm.mem.value, label: `Tidemark ${fmt(tm.mem.value)} · ${fmtAt(tm.mem.at)}` }
        : null,
      high: memHigh
        ? `Today's high ${fmt(memHigh.v)} at ${fmtAt(memHigh.t)}`
        : "No reading today yet",
      fmt,
      to: "performance" as Screen,
    },
    {
      name: "Network",
      range: "Last 24 h",
      value: latest ? fmtRate(latest.net_in).split(" ")[0] : "–",
      unit: latest ? `${fmtRate(latest.net_in).split(" ")[1]} ↓` : "",
      sub: latest
        ? `↑ ${fmtRate(latest.net_out)} up · ${fmt(movedToday("net_in"))} down today`
        : "",
      pts: series("net_in"),
      max: Math.max(1, ...(day?.points ?? []).map((m) => m.net_in)),
      mark: null,
      high: netHigh ? `Busiest ${fmtRate(netHigh.v)} at ${fmtAt(netHigh.t)}` : "Quiet so far today",
      fmt: fmtRate,
      to: "performance" as Screen,
    },
    {
      name: "Disk",
      range: "Last 7 days",
      value: diskTotal ? Math.round((diskTotal - diskNow) / 1e9).toString() : "–",
      unit: "GB free",
      sub: diskTotal ? `${fmt(diskNow)} of ${fmt(diskTotal)} used` : "Recorded hourly",
      pts: diskPts,
      max: diskPts.length ? Math.max(...diskPts.map((p) => p.v)) * 1.01 : 1,
      min: diskPts.length ? Math.min(...diskPts.map((p) => p.v)) * 0.98 : 0,
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
      to: "trends" as Screen,
    },
  ];

  const questions = [
    marks.find((m) => m.incident)
      ? `Why was my Mac slow ${fmtAt(marks.filter((m) => m.incident).slice(-1)[0].t).replace(/ \d.*/, "")}?`
      : "Is anything slowing my Mac down?",
    landed[0]
      ? `What can I clear in ${abbrev(landed[0].path, home)}?`
      : "Where can I free space safely?",
    "Is the disk filling faster than usual?",
  ];

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
            <span className={st?.level === "attention" ? "warnt" : "tidet"}>.</span>
          </h1>
          <p className="muted">{sub || "The first readings arrive in a few seconds."}</p>
        </div>

        <div className="cards3">
          {cards.map((c) => (
            <section className="card metric" key={c.name}>
              <div className="row between">
                <span className="eyebrow">{c.name}</span>
                <span className="muted small">{c.range}</span>
              </div>
              <div className="big">
                <span className="num">{c.value}</span>
                <span className="muted unit">{c.unit}</span>
              </div>
              <div className="muted small">{c.sub}</div>
              <Area
                pts={c.pts}
                max={c.max}
                min={c.min ?? 0}
                height={72}
                axis={false}
                mark={c.mark}
                fmt={c.fmt}
                fmtT={(t) => fmtAt(t)}
                name={c.name}
                label={`${c.name}, ${c.range.toLowerCase()}`}
              />
              <div className="row between small">
                <span className="muted">{c.high}</span>
                <button
                  className="link"
                  onClick={() => go(c.to)}
                  title={
                    c.to === "trends"
                      ? "See the disk over weeks and months"
                      : `See ${c.name.toLowerCase()} in detail`
                  }
                >
                  Details
                </button>
              </div>
            </section>
          ))}
        </div>

        <div className="cards2">
          <section className="card list">
            <div className="row between head">
              <span className="eyebrow">This week's tidemarks</span>
              <button className="link small" onClick={() => go("trends")} title="Open Trends (⌘4)">
                All trends
              </button>
            </div>
            {marks.length === 0 && (
              <div className="muted rowline">
                No incidents this week. Peaks appear here as they happen.
              </div>
            )}
            {marks.slice(-5).map((m, i) => (
              <div className="rowline" key={i}>
                <span className="mono muted when">{fmtAt(m.t)}</span>
                <span className="col">
                  <span className="row wrap">
                    <b>{m.what}</b>
                    {m.incident && <span className="badge warn">Incident · snapshot kept</span>}
                  </span>
                  <span className="muted small">{m.why}</span>
                </span>
              </div>
            ))}
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
                  : "Growth shows once two scans are a day or more apart."}
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
              title="Open the map with this week's growth marked (⌘3)"
            >
              Show the growth on the map →
            </button>
          </section>
        </div>

        <section className="askstrip">
          <span className="lbl">Ask Tidemark</span>
          {questions.map((q) => (
            <button className="chip" key={q} onClick={() => ask(q)} title="Ask this (⌘K opens Ask)">
              {q}
            </button>
          ))}
        </section>
      </div>
    </>
  );
}
