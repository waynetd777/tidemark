// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// The charts: one series each, in the tide colour, with the dashed tidemark line (the high-water
// mark of the period) that runs through the whole app. Drawn as SVG at the measured width so
// lines stay 2px and markers round.

import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";

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
}: AreaProps) {
  const [ref, W] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const H = height;
  const left = axis ? 44 : 0;
  const w = Math.max(0, W - left);
  const hi = Math.max(max, mark?.v ?? 0, ...pts.map((p) => p.v)) || 1;
  const lo = min;
  const t0 = pts.length ? pts[0].t : 0;
  const t1 = pts.length ? pts[pts.length - 1].t : 1;
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
    // x and y follow W, pts and the scale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pts, W, hi, lo, H]);
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
  return (
    <div
      className="area"
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
        <span className="tmlabel" style={{ top: y(mark.v) + 3 }}>
          {mark.label}
        </span>
      )}
      {peak && W > 0 && (
        <>
          <span className="dot peakdot" style={{ left: x(peak.t), top: y(peak.v) }} />
          <span
            className="peaklabel"
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
