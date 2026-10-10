// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// Performance: CPU and memory over a range with the week's tidemark and the range's peak,
// smaller tiles for swap, disk, network and heat, and what every app was doing at a chosen
// moment (the peak, or wherever the chart is clicked).

import { useMemo, useState, type ReactNode } from "react";
import { api } from "./api";
import { Area, Spark, type Pt } from "./charts";
import { incidentTitle } from "./incidents";
import { now, usePoll } from "./perf";
import type { Incident, Live, Minute } from "./types";
import { fmt, fmtAt, fmtRate, pct } from "./util";

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

export default function Performance({
  live,
  historyDays,
  bar,
  ask,
}: {
  live: Live | null;
  historyDays: number;
  bar: (left: ReactNode) => ReactNode;
  ask: (q: string) => void;
}) {
  const [range, setRange] = useState(1);
  const secs = RANGES[range].secs;
  const hist = usePoll(() => api.perf(secs, 480), 60_000, [secs]);
  const incidents = usePoll(() => api.incidents(14), 60_000);
  // The moment the snapshot is of: picked on a chart, else the range's CPU peak.
  const [picked, setPicked] = useState<{ t: number; why: string } | null>(null);
  const points = useMemo(() => hist?.points ?? [], [hist]);
  const bucket = points.length > 1 ? points[1].ts - points[0].ts : 60;
  const cpuPeak = peakOf(points, "cpu_max");
  const memPeak = peakOf(points, "mem_max");
  const at = picked ?? (cpuPeak ? { t: cpuPeak.t, why: "cpu" } : null);
  const apps = usePoll(
    () => (at ? api.appsBetween(at.t - 60, at.t + bucket) : Promise.resolve(null)),
    60_000,
    [at?.t, bucket],
  );
  const latest = live?.samples[live.samples.length - 1];
  const total = live?.mem_total || points[0]?.mem_total || 1;
  const tm = hist?.tidemarks ?? {};

  type Num = keyof Minute;
  const ser = (k: Num): Pt[] => points.map((m) => ({ t: m.ts, v: m[k] }));
  const atPoint = points.reduce<Minute | null>(
    (b, m) => (at && (!b || Math.abs(m.ts - at.t) < Math.abs(b.ts - at.t)) ? m : b),
    null,
  );
  const blameByMem = at?.why === "mem";
  const blame = (blameByMem ? apps?.by_mem : apps?.by_cpu) ?? [];
  const top = blame[0];
  const atValue = atPoint ? (blameByMem ? atPoint.mem_max : atPoint.cpu_max) : null;
  const allowed = RANGES.filter(
    (r) => r.secs <= Math.max(historyDays, 1) * 86400 || r.secs <= 86400,
  );

  const charts = [
    {
      name: "CPU",
      now: latest ? pct(latest.cpu) : "–",
      sub: `${live?.cores ?? "–"} cores${points.length ? ` · average ${pct(points.reduce((s, m) => s + m.cpu, 0) / points.length)}` : ""}`,
      pts: ser("cpu"),
      max: 100,
      fmt: pct,
      mark: tm.cpu
        ? { v: tm.cpu.value, label: `Tidemark ${pct(tm.cpu.value)} · ${fmtAt(tm.cpu.at)}` }
        : null,
      peak: cpuPeak ? { ...cpuPeak, label: `${pct(cpuPeak.v)} · ${fmtAt(cpuPeak.t)}` } : null,
      extra: (i: number) => <span>Peak {pct(points[i].cpu_max)}</span>,
      why: "cpu",
    },
    {
      name: "Memory",
      now: latest ? fmt(latest.mem) : "–",
      sub: `of ${fmt(total)} · pressure ${latest && latest.pressure >= 4 ? "critical" : latest && latest.pressure >= 2 ? "raised" : "normal"}`,
      pts: ser("mem"),
      max: total,
      fmt,
      mark: tm.mem
        ? { v: tm.mem.value, label: `Tidemark ${fmt(tm.mem.value)} · ${fmtAt(tm.mem.at)}` }
        : null,
      peak: memPeak ? { ...memPeak, label: `${fmt(memPeak.v)} · ${fmtAt(memPeak.t)}` } : null,
      extra: (i: number) => (
        <span>
          Peak {fmt(points[i].mem_max)} · swap {fmt(points[i].swap_max)}
        </span>
      ),
      why: "mem",
    },
  ];

  const tail = points.slice(-60);
  const high = (k: Num) => tail.reduce<Minute | null>((b, m) => (!b || m[k] > b[k] ? m : b), null);
  const swapHigh = tm.swap;
  const ioHigh = high("disk_max");
  // Network: download and upload as two small charts on one scale (not one chart with two axes),
  // what moved in the range, and each direction's peak.
  const moved = (k: "net_in" | "net_out") =>
    points.reduce((s, m) => s + m[k] * (m.minutes || 1) * 60, 0);
  const netTop = Math.max(1, ...points.map((m) => Math.max(m.net_in, m.net_out)));
  const netPeak = (k: "net_in" | "net_out") => {
    const m = points.reduce<Minute | null>((b, x) => (!b || x[k] > b[k] ? x : b), null);
    return m && m[k] > 0 ? { t: m.ts, v: m[k], label: `${fmtRate(m[k])} · ${fmtAt(m.ts)}` } : null;
  };
  const net = [
    { name: "Download", k: "net_in" as const, now: latest?.net_in, total: moved("net_in") },
    { name: "Upload", k: "net_out" as const, now: latest?.net_out, total: moved("net_out") },
  ];
  const tiles = [
    {
      name: "Swap",
      value: latest ? fmt(latest.swap) : "–",
      note: swapHigh ? `Week high ${fmt(swapHigh.value)}, ${fmtAt(swapHigh.at, false)}` : "",
      pts: tail.map((m) => ({ t: m.ts, v: m.swap })),
      max: Math.max(1e9, ...tail.map((m) => m.swap_max)),
    },
    {
      name: "Disk I/O",
      value: latest ? fmtRate(latest.disk_r + latest.disk_w) : "–",
      note: ioHigh ? `Peak ${fmtRate(ioHigh.disk_max)} at ${fmtAt(ioHigh.ts)}` : "",
      pts: tail.map((m) => ({ t: m.ts, v: m.disk_r + m.disk_w })),
      max: 1,
    },
    {
      name: "Thermal",
      value: latest ? (THERMAL[latest.thermal] ?? "Nominal") : "–",
      note: tail.some((m) => m.thermal >= 2)
        ? "Ran hot in this range"
        : "No throttling in this range",
      pts: tail.map((m) => ({ t: m.ts, v: m.thermal })),
      max: 3,
    },
  ];

  const pickIncident = (i: Incident) => {
    const age = now() - i.peak_at;
    const r = RANGES.findIndex((x) => x.secs > age);
    if (r >= 0) setRange(r);
    setPicked({ t: i.peak_at, why: i.kind === "cpu" ? "cpu" : "mem" });
  };

  return (
    <>
      {bar(
        <>
          <span className="ttl">Performance</span>
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
        <div className="perfmain">
          {charts.map((c) => (
            <section className="card chart" key={c.name}>
              <div className="row base wrap">
                <b className="cname">{c.name}</b>
                <span className="cnow">{c.now}</span>
                <span className="muted small">{c.sub}</span>
                <span className="grow" />
                <span className="muted small legend-tm">
                  <svg width="18" height="6" aria-hidden="true">
                    <line x1="0" x2="18" y1="3" y2="3" strokeDasharray="4 3" />
                  </svg>
                  Week's tidemark
                </span>
              </div>
              {c.pts.length ? (
                <Area
                  pts={c.pts}
                  max={c.max}
                  mark={c.mark}
                  peak={c.peak}
                  fmt={c.fmt}
                  fmtT={(t) => fmtAt(t)}
                  name={c.name}
                  extra={c.extra}
                  xLabels={xLabels(secs)}
                  domain={[now() - secs, now()]}
                  label={`${c.name} over the last ${RANGES[range].label}`}
                  onPick={(p) => setPicked({ t: p.t, why: c.why })}
                />
              ) : (
                <div className="empty small">Recording… the first minute appears shortly.</div>
              )}
            </section>
          ))}
          <section className="card chart">
            <div className="row base wrap">
              <b className="cname">Network</b>
              <span className="cnow">
                {latest ? `↓ ${fmtRate(latest.net_in)}  ↑ ${fmtRate(latest.net_out)}` : "–"}
              </span>
              <span className="muted small">
                {points.length
                  ? `${fmt(net[0].total)} down, ${fmt(net[1].total)} up in the last ${RANGES[range].label}`
                  : ""}
              </span>
            </div>
            <div className="netpair">
              {net.map((n) => (
                <div className="minw0" key={n.k}>
                  <div className="row between small">
                    <span className="eyebrow">{n.name}</span>
                    <span className="muted">{n.now != null ? `${fmtRate(n.now)} now` : ""}</span>
                  </div>
                  {points.length ? (
                    <Area
                      pts={ser(n.k)}
                      max={netTop}
                      height={100}
                      peak={netPeak(n.k)}
                      domain={[now() - secs, now()]}
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
          <div className="tiles">
            {tiles.map((t) => (
              <section className="card tile" key={t.name}>
                <span className="eyebrow">{t.name}</span>
                <b className="tv">{t.value}</b>
                <Spark
                  pts={t.pts}
                  max={t.max}
                  width={150}
                  height={26}
                  label={`${t.name}, recent`}
                />
                <span className="muted small">{t.note}</span>
              </section>
            ))}
          </div>
        </div>

        <aside className="card snap">
          <div>
            <span className="eyebrow">
              {at
                ? `Snapshot at ${fmtAt(at.t)}${!picked ? ` · the range's CPU peak` : ""}`
                : "Snapshot"}
            </span>
            {atValue != null && (
              <div className="big">
                <span className="num">{blameByMem ? fmt(atValue) : pct(atValue)}</span>
                <span className="muted">{blameByMem ? "memory in use" : "CPU"}</span>
              </div>
            )}
          </div>
          <div className="blame">
            {blame.length === 0 && (
              <div className="muted small">Nothing recorded at that moment.</div>
            )}
            {blame.slice(0, 6).map((p) => {
              const v = blameByMem ? p.mem : p.cpu;
              const hi = blameByMem ? blame[0].mem : blame[0].cpu;
              return (
                <div className="brow" key={p.app}>
                  <div className="row between">
                    <span className="ell">{p.app}</span>
                    <b>{blameByMem ? fmt(v) : pct(v)}</b>
                  </div>
                  <span className="bar">
                    <i style={{ width: `${hi ? (100 * v) / hi : 0}%` }} />
                  </span>
                  <span className="muted small">
                    {blameByMem ? `${pct(p.cpu)} CPU` : `${fmt(p.mem)} memory`}
                  </span>
                </div>
              );
            })}
          </div>
          {atValue != null && !blameByMem && (
            <div className="note">
              {atValue >= 90
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
              title="Put this moment to Ask (⌘K)"
              onClick={() =>
                ask(
                  at
                    ? `Why was ${blameByMem ? "memory" : "the CPU"} ${atValue != null ? (blameByMem ? `at ${fmt(atValue)}` : `at ${pct(atValue)}`) : "high"} at ${fmtAt(at.t)}${top ? `, with ${top.app} on top` : ""}?`
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
