// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// Trends: the disk over weeks and months with its high-water mark and its big jumps, the folders
// that rose, when the Mac is usually busy, and the range's highs. Disk totals go back a year;
// performance goes back as far as the history setting keeps it (14 days at most).

import { useState, type ReactNode } from "react";
import { Labelled } from "./icons";
import { metricOf } from "./metrics";
import { api } from "./api";
import { Area, Heatmap, Spark, type Pt } from "./charts";
import { now, usePoll } from "./perf";
import type { DiskPoint, Growth, Screen } from "./types";
import { abbrev, fmt, fmtAt, fmtDelta, pct } from "./util";

const RANGES = [
  { label: "14 days", days: 14 },
  { label: "30 days", days: 30 },
  { label: "3 months", days: 91 },
  { label: "12 months", days: 365 },
];

function dateLabel(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

/// Day-on-day rises of 2 GB or more, biggest first, with the day they happened.
function jumps(points: DiskPoint[]): { ts: number; delta: number }[] {
  const byDay = new Map<string, DiskPoint>();
  for (const p of points) byDay.set(new Date(p.ts * 1000).toDateString(), p);
  const days = [...byDay.values()];
  const out: { ts: number; delta: number }[] = [];
  for (let i = 1; i < days.length; i++) {
    const d = days[i].used - days[i - 1].used;
    if (d >= 2e9) out.push({ ts: days[i].ts, delta: d });
  }
  return out.sort((a, b) => b.delta - a.delta).slice(0, 3);
}

/// One folder's size over the scans in the history, as a small line.
function FolderSpark({ path, days }: { path: string; days: number }) {
  const h = usePoll(() => api.folderHistory(path, days), 300_000, [path, days]);
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

export default function Trends({
  home,
  historyDays,
  bar,
  go,
  ask,
}: {
  home: string;
  historyDays: number;
  bar: (left: ReactNode) => ReactNode;
  go: (s: Screen) => void;
  ask: (q: string) => void;
}) {
  const [range, setRange] = useState(1);
  const days = RANGES[range].days;
  const trend = usePoll(() => api.diskTrend(days), 300_000, [days]);
  const growth = usePoll(() => api.growth("/", days), 300_000, [days]);
  const heat = usePoll(api.cpuByHour, 600_000);
  const perfDays = Math.min(days, historyDays);
  const perf = usePoll(() => api.perf(perfDays * 86400, 1, perfDays), 300_000, [perfDays]);

  const pts: Pt[] = (trend?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const first = trend?.points[0];
  const last = trend?.points[trend.points.length - 1];
  const delta = first && last ? last.used - first.used : 0;
  const spanDays = first && last ? Math.max(1, (last.ts - first.ts) / 86400) : 1;
  const perDay = delta / spanDays;
  const monthsLeft = last && perDay > 1e7 ? (last.total - last.used) / perDay / 30.4 : null;
  const big = jumps(trend?.points ?? []);
  const high = trend?.high;
  const lo = pts.length ? Math.min(...pts.map((p) => p.v)) : 0;
  const hi = pts.length ? Math.max(...pts.map((p) => p.v)) : 1;
  const pad = Math.max((hi - lo) * 0.15, 2e9);
  const rose = (growth?.landed ?? []).filter((g: Growth) => g.delta > 0).slice(0, 6);
  const tm = perf?.tidemarks ?? {};

  // The busiest hours, in a sentence.
  let busiest = "";
  if (heat) {
    const cells: { d: number; h: number; v: number }[] = [];
    heat.forEach((row, d) => row.forEach((v, h) => v != null && cells.push({ d, h, v })));
    cells.sort((a, b) => b.v - a.v);
    const wd = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    const weekdays = cells.slice(0, 8).filter((c) => c.d < 5).length;
    if (cells.length > 24 && cells[0])
      busiest =
        weekdays >= 6
          ? `Busiest on weekdays around ${String(cells[0].h).padStart(2, "0")}:00.`
          : `Busiest on ${wd[cells[0].d]} around ${String(cells[0].h).padStart(2, "0")}:00.`;
  }

  const marks = [
    {
      name: "CPU high",
      value: tm.cpu ? pct(tm.cpu.value) : "–",
      when: tm.cpu ? fmtAt(tm.cpu.at) : "",
    },
    {
      name: "Memory high",
      value: tm.mem ? fmt(tm.mem.value) : "–",
      when: tm.mem ? fmtAt(tm.mem.at) : "",
    },
    {
      name: "Swap high",
      value: tm.swap ? fmt(tm.swap.value) : "–",
      when: tm.swap ? fmtAt(tm.swap.at) : "",
    },
    { name: "Disk high", value: high ? fmt(high.used) : "–", when: high ? dateLabel(high.ts) : "" },
  ];

  // The chart spans the range asked for, so its labels are the range's dates.
  const t1 = now();
  const t0 = t1 - days * 86400;
  const xl = Array.from({ length: 5 }, (_, i) =>
    i === 4 ? "today" : dateLabel(t0 + ((t1 - t0) * i) / 4),
  );

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
                onClick={() => setRange(i)}
                title={`The disk over the last ${r.label}`}
              >
                {r.label}
              </button>
            ))}
          </div>
          {first && last && (
            <span className="muted small">
              {dateLabel(first.ts)} – {dateLabel(last.ts)}
            </span>
          )}
          <div className="grow" />
        </>,
      )}
      <div className="page">
        <section className="card chart">
          <div className="row base wrap">
            <b className="cname">
              <Labelled m="disk">Disk used</Labelled>
            </b>
            <span className="cnow">{pts.length > 1 ? fmtDelta(delta) : "–"}</span>
            {first && last && (
              <span className="muted small">
                over {Math.round(spanDays)} days, from {fmt(first.used)} to {fmt(last.used)}
                {Math.abs(perDay) > 1e7 ? ` · about ${fmt(Math.abs(perDay))} a day` : ""}
              </span>
            )}
            <span className="grow" />
            {monthsLeft != null && (
              <span className="muted small">
                At this rate the disk is full in about{" "}
                {monthsLeft >= 24
                  ? `${Math.round(monthsLeft / 12)} years`
                  : `${Math.max(1, Math.round(monthsLeft))} months`}
              </span>
            )}
          </div>
          <div className="trendwrap">
            <div className="grow minw0">
              {pts.length > 1 ? (
                <Area
                  pts={pts}
                  max={hi + pad}
                  min={Math.max(0, lo - pad)}
                  height={180}
                  mark={
                    high
                      ? {
                          v: high.used,
                          label: `Tidemark ${fmt(high.used)} · ${dateLabel(high.ts)}`,
                        }
                      : null
                  }
                  fmt={fmt}
                  fmtT={(t) => (days > 14 ? dateLabel(t) : fmtAt(t))}
                  name="Disk used"
                  xLabels={xl}
                  domain={[t0, t1]}
                  label={`Disk used over the last ${RANGES[range].label}`}
                />
              ) : (
                <div className="empty small">
                  The disk's used space is recorded every hour. The trend appears after the first
                  day.
                </div>
              )}
            </div>
            {(big.length > 0 || high) && (
              <ol className="pins">
                {big.map((j) => (
                  <li key={j.ts}>
                    <b>
                      {dateLabel(j.ts)} · {fmtDelta(j.delta)}.
                    </b>{" "}
                    <span className="muted">
                      The biggest one-day rise{big.length > 1 ? "s" : ""} in the range.
                    </span>
                  </li>
                ))}
                {high && last && (
                  <li>
                    <b>
                      {dateLabel(high.ts)} · {fmt(high.used)}, the high-water mark.
                    </b>{" "}
                    <span className="muted">
                      {last.used < high.used - 1e9
                        ? `${fmt(high.used - last.used)} freed since.`
                        : "It is still near there."}
                    </span>
                  </li>
                )}
              </ol>
            )}
          </div>
        </section>

        <div className="cards2">
          <section className="card list">
            <div className="row between head">
              <span className="eyebrow">Top folders that rose · {RANGES[range].label}</span>
              <button className="link small" onClick={() => go("disk")} title="Open the map (⌘3)">
                Open the map
              </button>
            </div>
            {rose.length === 0 && (
              <div className="muted rowline">
                {days > 14
                  ? "Longer ranges compare the daily top folders. They fill in as the days pass."
                  : "Nothing rose by more than 100 MB."}
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
          <section className="card">
            <div className="row between head">
              <span className="eyebrow">When it gets busy · CPU by hour</span>
              <span className="muted small">Last {historyDays} days</span>
            </div>
            {heat ? <Heatmap grid={heat} fmt={(v) => `${pct(v)} CPU on average`} /> : null}
            {busiest && <p className="muted small">{busiest}</p>}
          </section>
        </div>

        <div className="tiles">
          {marks.map((m) => (
            <section className="card tile" key={m.name}>
              <span className="eyebrow">
                <Labelled m={metricOf(m.name)}>{m.name}</Labelled>
              </span>
              <b className="tv big2">{m.value}</b>
              <span className="muted small">{m.when}</span>
            </section>
          ))}
        </div>

        <section className="askstrip">
          <span className="lbl">Ask Tidemark</span>
          {[
            "Will I run out of disk this year?",
            `What grew most in the last ${RANGES[range].label}?`,
            "When is my Mac usually busiest, and why?",
          ].map((q) => (
            <button className="chip" key={q} onClick={() => ask(q)} title="Ask this (⌘K opens Ask)">
              {q}
            </button>
          ))}
        </section>
      </div>
    </>
  );
}
