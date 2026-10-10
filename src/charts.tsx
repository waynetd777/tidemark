// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// The charts: one series each, in the tide colour, with the dashed tidemark line (the high-water
// mark of the period) that runs through the whole app. Drawn as SVG at the measured width so
// lines stay 2px and markers round.

import { useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Gap } from "./perf";
import { fmtAt, fmtMins } from "./util";

export interface Pt {
  /// Unix seconds.
  t: number;
  v: number;
}

/// Splits a series where readings stop for longer than `gap` seconds (the Mac slept, the app
/// was not running): a gap is drawn as a gap, never as a straight line across it.
function runs(pts: Pt[], gap: number): Pt[][] {
  const out: Pt[][] = [];
  let cur: Pt[] = [];
  for (const p of pts) {
    if (cur.length && p.t - cur[cur.length - 1].t > gap) {
      out.push(cur);
      cur = [];
    }
    cur.push(p);
  }
  if (cur.length) out.push(cur);
  return out;
}

/// The typical spacing of a series, for spotting gaps.
function step(pts: Pt[]): number {
  if (pts.length < 2) return 60;
  const d = pts
    .slice(1)
    .map((p, i) => p.t - pts[i].t)
    .sort((a, b) => a - b);
  return d[Math.floor(d.length / 2)] || 60;
}

function useWidth<T extends HTMLElement>(): [React.RefObject<T | null>, number] {
  const ref = useRef<T>(null);
  const [w, setW] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(el.clientWidth));
    ro.observe(el);
    setW(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  return [ref, w];
}

/// A small line with its area, and the tidemark as a dashed line. For the tray and tiles.
export function Spark({
  pts,
  min = 0,
  max,
  mark,
  width = 96,
  height = 26,
  tone = "tide",
  label,
}: {
  pts: Pt[];
  min?: number;
  max: number;
  mark?: number | null;
  width?: number;
  height?: number;
  tone?: "tide" | "warn";
  label: string;
}) {
  const lo = Math.min(min, ...pts.map((p) => p.v));
  const hi = Math.max(max, mark ?? 0, ...pts.map((p) => p.v));
  const span = hi - lo || 1;
  const t0 = pts.length ? pts[0].t : 0;
  const t1 = pts.length ? pts[pts.length - 1].t : 1;
  const x = (t: number) => ((t - t0) / (t1 - t0 || 1)) * width;
  const y = (v: number) => height - 1 - ((v - lo) / span) * (height - 2);
  const paths = runs(pts, step(pts) * 4).map((r) => {
    const line = "M" + r.map((p) => `${x(p.t).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" L");
    return {
      line,
      area: `${line} L${x(r[r.length - 1].t).toFixed(1)} ${height} L${x(r[0].t).toFixed(1)} ${height} Z`,
    };
  });
  const color = tone === "warn" ? "var(--warn)" : "var(--tide)";
  return (
    <svg
      className="spark"
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={label}
      data-tip={label}
    >
      {paths.map((p, i) => (
        <g key={i}>
          <path d={p.area} fill={color} fillOpacity={0.14} />
          <path d={p.line} fill="none" stroke={color} strokeWidth={1.6} strokeLinejoin="round" />
        </g>
      ))}
      {mark != null && mark > 0 && (
        <line
          x1={0}
          x2={width}
          y1={y(mark)}
          y2={y(mark)}
          stroke="var(--mark)"
          strokeWidth={1}
          strokeDasharray="3 2.5"
          opacity={0.5}
        />
      )}
    </svg>
  );
}

export interface AreaProps {
  pts: Pt[];
  /// The top of the scale (100 for CPU, total memory, ...). The data can push it higher.
  max: number;
  min?: number;
  height?: number;
  /// The period's high-water mark: a dashed line across, labelled at the right.
  mark?: { v: number; label: string } | null;
  /// A point to call out (the range's peak), with its label.
  peak?: { t: number; v: number; label: string } | null;
  /// Value and time formatting for the axis and the hover readout.
  fmt: (v: number) => string;
  fmtT: (t: number) => string;
  /// Labels under the chart, left to right.
  xLabels?: string[];
  /// What the readout calls the value ("CPU", "Memory used").
  name: string;
  /// Extra lines for the readout at a point (the bucket's peak, say).
  extra?: (i: number) => ReactNode;
  label: string;
  onPick?: (p: Pt) => void;
  /// Draw the y-axis labels (top and middle) at the left.
  axis?: boolean;
  /// Each point's peak (the busiest reading inside a minute), drawn as a faint band behind the
  /// line of averages, so a short spike, the peak dot and the tidemark have something to sit on.
  hi?: Pt[];
  /// Stretches with no readings, shaded and labelled for what they were (gapsOf in perf.ts).
  gaps?: Gap[];
  /// The time span across the chart (unix seconds): the range asked for, so a reading sits at
  /// its real time and the labels under the chart are true even when the record is shorter.
  domain?: [number, number];
}

/// A line over its area, with a crosshair that follows the pointer, snaps to the nearest
/// reading and shows it; the tidemark; and the peak marked with a dot and its value.
export function Area({
  pts,
  max,
  min = 0,
  height = 170,
  mark,
  peak,
  fmt,
  fmtT,
  xLabels,
  name,
  extra,
  label,
  onPick,
  axis = true,
  domain,
  hi: band,
  gaps,
}: AreaProps) {
  const hatch = useId();
  const [ref, W] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const H = height;
  const left = axis ? 44 : 0;
  const w = Math.max(0, W - left);
  const hi =
    Math.max(max, mark?.v ?? 0, ...pts.map((p) => p.v), ...(band ?? []).map((p) => p.v)) || 1;
  const lo = min;
  const t0 = domain?.[0] ?? (pts.length ? pts[0].t : 0);
  const t1 = domain?.[1] ?? (pts.length ? pts[pts.length - 1].t : 1);
  const x = (t: number) => left + ((t - t0) / (t1 - t0 || 1)) * w;
  const y = (v: number) => H - 1 - ((v - lo) / (hi - lo || 1)) * (H - 4);
  const paths = useMemo(() => {
    return runs(pts, step(pts) * 4).map((r) => {
      const line = "M" + r.map((p) => `${x(p.t).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" L");
      return {
        line,
        area: `${line} L${x(r[r.length - 1].t).toFixed(1)} ${H} L${x(r[0].t).toFixed(1)} ${H} Z`,
        single: r.length === 1 ? r[0] : null,
      };
    });
    // x and y follow W, pts, the time span and the scale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pts, W, hi, lo, H, t0, t1]);
  const bandPaths = useMemo(() => {
    if (!band?.length) return [];
    return runs(band, step(band) * 4).map((r) => {
      const line = "M" + r.map((p) => `${x(p.t).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" L");
      return `${line} L${x(r[r.length - 1].t).toFixed(1)} ${H} L${x(r[0].t).toFixed(1)} ${H} Z`;
    });
    // x and y follow W, the band, the time span and the scale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [band, W, hi, lo, H, t0, t1]);
  const nearest = (clientX: number, el: HTMLElement) => {
    if (!pts.length) return null;
    const r = el.getBoundingClientRect();
    const t = t0 + ((clientX - r.left - left) / (w || 1)) * (t1 - t0);
    let best = 0;
    for (let i = 1; i < pts.length; i++)
      if (Math.abs(pts[i].t - t) < Math.abs(pts[best].t - t)) best = i;
    return best;
  };
  const hp = hover != null ? pts[hover] : null;
  const tipLeft = hp ? Math.min(Math.max(x(hp.t), left + 70), W - 70) : 0;
  // A peak at the top of the chart has its label below the dot, inside the card.
  const peakBelow = peak ? y(peak.v) < 28 : false;
  return (
    <div className="area">
      <div
        className="plot"
        ref={ref}
        style={{ height: H }}
        onPointerMove={(e) => setHover(nearest(e.clientX, e.currentTarget))}
        onPointerLeave={() => setHover(null)}
        onClick={() => {
          if (hp && onPick) onPick(hp);
        }}
      >
        {W > 0 && (
          <svg width={W} height={H} role="img" aria-label={label}>
            <line className="gridl" x1={left} x2={W} y1={0.5} y2={0.5} />
            <line className="gridl" x1={left} x2={W} y1={H / 2} y2={H / 2} />
            <line className="basel" x1={left} x2={W} y1={H - 0.5} y2={H - 0.5} />
            <defs>
              <pattern id={hatch} width="6" height="6" patternUnits="userSpaceOnUse">
                <path d="M0 6L6 0" className="hatchl" />
              </pattern>
            </defs>
            {(gaps ?? []).map((g, i) => {
              const gx = Math.max(left, x(g.from));
              const gw = Math.max(0, Math.min(W, x(g.to)) - gx);
              if (gw < 1) return null;
              const what = g.kind === "sleep" ? "Asleep" : "Not recording";
              return (
                <g key={`g${i}`}>
                  <rect
                    className={`gap ${g.kind}`}
                    x={gx}
                    y={0}
                    width={gw}
                    height={H}
                    fill={g.kind === "off" ? `url(#${hatch})` : undefined}
                    data-tip={`${what} · ${fmtAt(g.from)} to ${fmtAt(g.to)} · ${fmtMins(Math.round((g.to - g.from) / 60))}`}
                  />
                  {gw >= 64 && (
                    <text className="gaplabel" x={gx + gw / 2} y={14} textAnchor="middle">
                      {what}
                    </text>
                  )}
                </g>
              );
            })}
            {bandPaths.map((d, i) => (
              <path key={`b${i}`} d={d} fill="var(--tide)" fillOpacity={0.1} />
            ))}
            {paths.map((p, i) =>
              p.single ? (
                <circle key={i} cx={x(p.single.t)} cy={y(p.single.v)} r={2} fill="var(--tide)" />
              ) : (
                <g key={i}>
                  <path d={p.area} fill="var(--tide)" fillOpacity={0.12} />
                  <path
                    d={p.line}
                    fill="none"
                    stroke="var(--tide)"
                    strokeWidth={2}
                    strokeLinejoin="round"
                  />
                </g>
              ),
            )}
            {mark && mark.v > 0 && (
              <line
                className="tm"
                x1={left}
                x2={W}
                y1={y(mark.v)}
                y2={y(mark.v)}
                strokeDasharray="4 3"
              />
            )}
            {/* Now: the right-hand end of a chart that runs up to the present. */}
            {domain && (
              <line
                className="nowl"
                x1={x(domain[1]) - 0.5}
                x2={x(domain[1]) - 0.5}
                y1={0}
                y2={H}
                strokeDasharray="2 3"
              />
            )}
            {peak && <line className="peakl" x1={x(peak.t)} x2={x(peak.t)} y1={0} y2={H} />}
            {hp && <line className="cross" x1={x(hp.t)} x2={x(hp.t)} y1={0} y2={H} />}
          </svg>
        )}
        {axis && W > 0 && (
          <>
            <span className="ax" style={{ top: 0 }}>
              {fmt(hi)}
            </span>
            <span className="ax" style={{ top: H / 2 }}>
              {fmt((hi + lo) / 2)}
            </span>
          </>
        )}
        {mark && mark.v > 0 && W > 0 && (
          <span className="tmlabel" style={{ top: y(mark.v) + 3, left: left + 4 }}>
            {mark.label}
          </span>
        )}
        {peak && W > 0 && (
          <>
            <span className="dot peakdot" style={{ left: x(peak.t), top: y(peak.v) }} />
            <span
              className={`peaklabel${peakBelow ? " below" : ""}`}
              style={{
                left: Math.min(Math.max(x(peak.t), left + 50), W - 50),
                top: y(peak.v),
              }}
            >
              {peak.label}
            </span>
          </>
        )}
        {hp && (
          <>
            <span className="dot hoverdot" style={{ left: x(hp.t), top: y(hp.v) }} />
            <div className="readout" style={{ left: tipLeft }}>
              <b>{fmt(hp.v)}</b>
              <span>
                {name} · {fmtT(hp.t)}
              </span>
              {extra?.(hover!)}
            </div>
          </>
        )}
      </div>
      {xLabels && (
        <div className="xl" style={{ marginLeft: left }}>
          {xLabels.map((l, i) => (
            <span key={i}>{l}</span>
          ))}
        </div>
      )}
    </div>
  );
}

/// Weekday-by-hour cells, shaded by one hue from quiet (light) to busy (dark), each with its
/// own hover readout.
export function Heatmap({ grid, fmt }: { grid: (number | null)[][]; fmt: (v: number) => string }) {
  const days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  const vals = grid.flat().filter((v): v is number => v != null);
  const hi = Math.max(1, ...vals);
  const [tip, setTip] = useState<{ d: number; h: number } | null>(null);
  const v = tip ? grid[tip.d]?.[tip.h] : null;
  return (
    <div className="heat" onPointerLeave={() => setTip(null)}>
      {grid.map((row, d) => (
        <div className="hrow" key={d}>
          <span className="hd">{days[d]}</span>
          <div className="cells">
            {row.map((c, h) => (
              <span
                key={h}
                className={`cell${c == null ? " none" : ""}${tip?.d === d && tip?.h === h ? " on" : ""}`}
                style={
                  c == null
                    ? undefined
                    : {
                        background: `color-mix(in srgb, var(--tide) ${Math.round(8 + (c / hi) * 92)}%, var(--panel))`,
                      }
                }
                onPointerEnter={() => setTip({ d, h })}
                tabIndex={c == null ? undefined : -1}
                aria-label={c == null ? undefined : `${days[d]} ${h}:00, ${fmt(c)}`}
              />
            ))}
          </div>
        </div>
      ))}
      <div className="hx">
        <span>00</span>
        <span>06</span>
        <span>12</span>
        <span>18</span>
        <span>23</span>
      </div>
      <div className="hscale">
        <span>Quiet</span>
        {[10, 30, 55, 80, 100].map((p) => (
          <i
            key={p}
            style={{ background: `color-mix(in srgb, var(--tide) ${p}%, var(--panel))` }}
          />
        ))}
        <span>Busy</span>
        <span className="grow" />
        {tip && (
          <span className="heattip">
            <b>{v == null ? "No record" : fmt(v)}</b> {days[tip.d]} {String(tip.h).padStart(2, "0")}
            :00
          </span>
        )}
      </div>
    </div>
  );
}

/// What a chart's marks mean: its line (always), the band of each minute's peak and the dashed
/// tidemark where the chart has them.
export function Legend({
  line,
  band,
  mark,
  gaps,
}: {
  line: string;
  band?: string;
  mark?: string;
  /// The chart's gaps: a key for each kind it has.
  gaps?: Gap[];
}) {
  return (
    <span className="chartlegend muted small">
      <span className="lk">
        <svg width="18" height="6" aria-hidden="true">
          <line className="lkline" x1="0" x2="18" y1="3" y2="3" />
        </svg>
        {line}
      </span>
      {band && (
        <span className="lk">
          <i className="bandkey" aria-hidden="true" />
          {band}
        </span>
      )}
      {mark && (
        <span className="lk">
          <svg width="18" height="6" aria-hidden="true">
            <line className="lkmark" x1="0" x2="18" y1="3" y2="3" strokeDasharray="4 3" />
          </svg>
          {mark}
        </span>
      )}
      {gaps?.some((g) => g.kind === "sleep") && (
        <span className="lk">
          <i className="sleepkey" aria-hidden="true" />
          Asleep
        </span>
      )}
      {gaps?.some((g) => g.kind === "off") && (
        <span className="lk">
          <i className="offkey" aria-hidden="true" />
          Not recording
        </span>
      )}
    </span>
  );
}
