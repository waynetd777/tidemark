// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Performance: CPU and memory over a range with the week's tidemark and the range's peak,
// smaller tiles for swap, disk, network and heat, and what every app was doing at a chosen
// moment (the peak, or wherever the chart is clicked).

import { useAssistant } from "./assistant";
import { useMemo, useState, type ReactNode } from "react";
import { Labelled } from "./icons";
import { metricOf } from "./metrics";
import type { AskDetail } from "./Ask";
import { AskButton } from "./AskButton";
import { flaggedDetail } from "./askDetails";
import { flagFor } from "./status";
import { api } from "./api";
import { Area, Legend, Spark, type Pt } from "./charts";
import { crashGroups, crashTitle, incidentTitle } from "./incidents";
import { gapsOf, now, upToNow, usePoll } from "./perf";
import { index, useRemembered } from "./remember";
import type { Incident, Live, Minute } from "./types";
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

export default function Performance({
  live,
  historyDays,
  bar,
  ask,
}: {
  live: Live | null;
  historyDays: number;
  bar: (left: ReactNode) => ReactNode;
  ask: (q: string, detail?: AskDetail) => void;
}) {
  const assistant = useAssistant();
  const [chosen, setRange] = useRemembered("performance.range", 1, index(RANGES.length));
  // Ranges longer than the history kept are not offered; a remembered one is held to what is.
  const allowed = RANGES.filter(
    (r) => r.secs <= Math.max(historyDays, 1) * 86400 || r.secs <= 86400,
  );
  const range = Math.min(chosen, allowed.length - 1);
  const secs = RANGES[range].secs;
  const hist = usePoll(() => api.perf(secs, 480), 60_000, [secs]);
  const incidents = usePoll(() => api.incidents(14), 60_000);
  // The moment the snapshot is of: picked on a chart, else the range's CPU peak.
  const [picked, setPicked] = useState<{ t: number; why: string } | null>(null);
  const points = useMemo(() => hist?.points ?? [], [hist]);
  // What one point stands for, as the history folds the range (perf_history's 480 points).
  const bucket = Math.max(60, Math.floor(secs / 480));
  const per = bucketWord(bucket);
  const cpuPeak = peakOf(points, "cpu_max");
  const memPeak = peakOf(points, "mem_max");
  const at = picked ?? (cpuPeak ? { t: cpuPeak.t, why: "cpu" } : null);
  const apps = usePoll(
    () => (at ? api.appsBetween(at.t - 60, at.t + bucket) : Promise.resolve(null)),
    60_000,
    [at?.t, bucket],
  );
  // The range's energy: what used the most, and the times the Mac slept.
  const rangeApps = usePoll(() => api.appsBetween(now() - secs, now()), 60_000, [secs]);
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
  const atPoint = points.reduce<Minute | null>(
    (b, m) => (at && (!b || Math.abs(m.ts - at.t) < Math.abs(b.ts - at.t)) ? m : b),
    null,
  );
  const blameByMem = at?.why === "mem";
  const blame = (blameByMem ? apps?.by_mem : apps?.by_cpu) ?? [];
  const top = blame[0];
  const atValue = atPoint ? (blameByMem ? atPoint.mem_max : atPoint.cpu_max) : null;
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
      why: "cpu",
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
            // macOS doesn't say which apps use the GPU, so a moment on it has no snapshot.
            why: "",
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
      why: "mem",
    },
  ];

  const tail = points.slice(-60);
  // The tiles' span: their last 60 points, up to now.
  const tailDomain: [number, number] = [tail.length ? tail[0].ts : now() - secs, now()];
  const swapHigh = tm.swap;
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
  const tiles = [
    {
      name: "Swap",
      value: latest ? fmt(latest.swap) : "–",
      note: swapHigh ? `Week high ${fmt(swapHigh.value)}, ${fmtAt(swapHigh.at, false)}` : "",
      pts: tail.map((m) => ({ t: m.ts, v: m.swap })),
      max: Math.max(1e9, ...tail.map((m) => m.swap_max)),
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
    const r = allowed.findIndex((x) => x.secs > age);
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
            <section className="card chart" key={c.name} data-focus={c.name.toLowerCase()}>
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
                <span className="muted small">{c.sub}</span>
                <span className="grow" />
                <Legend
                  line={`${c.name}, ${per} average`}
                  band={per === "minute" ? "Each minute's peak" : `Each ${per} peak`}
                  mark="Week's tidemark"
                  also={"also" in c ? c.also : undefined}
                  gaps={gapsOf(
                    c.pts.map((p) => p.t),
                    [now() - secs, now()],
                    sleeps ?? [],
                  )}
                />
              </div>
              {c.pts.length ? (
                <Area
                  tone={flagFor(live?.status, c.name)?.tone}
                  pts={c.pts}
                  hi={c.band}
                  max={c.max}
                  mark={c.mark}
                  peak={c.peak}
                  fmt={c.fmt}
                  fmtT={(t) => fmtAt(t)}
                  name={c.name}
                  extra={c.extra}
                  also={"also" in c ? c.also : undefined}
                  xLabels={xLabels(secs)}
                  domain={[now() - secs, now()]}
                  sleeps={sleeps ?? []}
                  label={`${c.name} over the last ${RANGES[range].label}`}
                  onPick={c.why ? (p) => setPicked({ t: p.t, why: c.why }) : undefined}
                />
              ) : (
                <div className="empty small">Recording… the first minute appears shortly.</div>
              )}
            </section>
          ))}
          <section className="card chart" data-focus="disk i/o">
            <div className="row base wrap">
              <b className="cname">
                <Labelled m="diskio">Disk I/O</Labelled>
              </b>
              <span className="cnow">
                {latest
                  ? `${fmtRate(latest.disk_r)} read · ${fmtRate(latest.disk_w)} written`
                  : "–"}
              </span>
              <span className="muted small">
                {points.length
                  ? `${fmt(io[0].total)} read, ${fmt(io[1].total)} written in the last ${RANGES[range].label}`
                  : ""}
              </span>
              <span className="grow" />
              <Legend
                line={`Average per ${per.replace("-minute", " minutes")}, each way`}
                gaps={gaps}
              />
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
                      pts={ser(n.k)}
                      max={ioTop}
                      height={100}
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
              <Legend
                line={`Average per ${per.replace("-minute", " minutes")}, each way`}
                gaps={gaps}
              />
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
                      pts={ser(n.k)}
                      max={netTop}
                      height={100}
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
                    height={120}
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
                                all.reduce((t, x) => t + x.energy, 0),
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
                  Each app's share of the energy your apps used. The display and macOS itself use
                  more, and only your own apps can be measured.
                </span>
              </div>
            )}
          </section>
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
                    pts={pts}
                    max={hi + pad}
                    min={Math.max(0, lo - pad)}
                    height={120}
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
          <div className="tiles">
            {tiles.map((t) => (
              <section className="card tile" key={t.name} data-focus={t.name.toLowerCase()}>
                <span className="eyebrow">
                  <Labelled m={metricOf(t.name)}>{t.name}</Labelled>
                </span>
                <b className="tv">{t.value}</b>
                <Spark
                  pts={t.pts}
                  max={t.max}
                  sleeps={sleeps ?? []}
                  domain={tailDomain}
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
              disabled={!assistant.available}
              title={
                assistant.available
                  ? "Put this moment to Ask (⌘K)"
                  : (assistant.reason ?? undefined)
              }
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
