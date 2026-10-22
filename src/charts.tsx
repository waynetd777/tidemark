// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The charts: one series each, in the tide colour, with the dashed tidemark line (the high-water
// mark of the period) that runs through the whole app. Drawn as SVG at the measured width so
// lines stay 2px and markers round.

import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { gapMin, gapsOf, type Gap } from "./perf";
import { toneColor, type Tone } from "./status";
import type { Sleep } from "./types";
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

/// The same readings, point for point.
function samePts(a: Pt[] | undefined, b: Pt[] | undefined): boolean {
  if (a === b) return true;
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i].t !== b[i].t || a[i].v !== b[i].v) return false;
  return true;
}

/// `v`, or the one kept from an earlier render when `same` says nothing changed. The screens build
/// their series afresh on every render; this lets the paths drawn from them be kept until a
/// reading really changes.
function useSame<T>(v: T, same: (a: T, b: T) => boolean): T {
  const [kept, setKept] = useState(v);
  if (v === kept || same(v, kept)) return kept;
  setKept(v);
  return v;
}

/// Where the picked moment's line starts: below its chip and the chip's arrow.
const PICK_TOP = 26;

const sameAlso = (a: AreaProps["also"], b: AreaProps["also"]) =>
  a === b ||
  (!!a &&
    !!b &&
    a.length === b.length &&
    a.every((x, i) => x.label === b[i].label && x.dash === b[i].dash && samePts(x.pts, b[i].pts)));

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
  sleeps,
  domain,
}: {
  pts: Pt[];
  min?: number;
  max: number;
  mark?: number | null;
  width?: number;
  height?: number;
  tone?: Tone;
  label: string;
  /// The recorded sleeps: with `domain`, its gaps are shaded as on the big charts.
  sleeps?: Sleep[];
  /// The time span across the line, so gaps and readings sit at their real times.
  domain?: [number, number];
}) {
  const hatch = useId();
  const gaps =
    sleeps && domain
      ? gapsOf(
          pts.map((p) => p.t),
          domain,
          sleeps,
        )
      : [];
  const lo = Math.min(min, ...pts.map((p) => p.v));
  const hi = Math.max(max, mark ?? 0, ...pts.map((p) => p.v));
  const span = hi - lo || 1;
  const t0 = domain?.[0] ?? (pts.length ? pts[0].t : 0);
  const t1 = domain?.[1] ?? (pts.length ? pts[pts.length - 1].t : 1);
  const x = (t: number) => ((t - t0) / (t1 - t0 || 1)) * width;
  const y = (v: number) => height - 1 - ((v - lo) / span) * (height - 2);
  const paths = runs(pts, gapMin(step(pts))).map((r) => {
    const line = "M" + r.map((p) => `${x(p.t).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" L");
    return {
      line,
      area: `${line} L${x(r[r.length - 1].t).toFixed(1)} ${height} L${x(r[0].t).toFixed(1)} ${height} Z`,
    };
  });
  const color = toneColor(tone);
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
      <defs>
        <pattern id={hatch} width="5" height="5" patternUnits="userSpaceOnUse">
          <path d="M0 5L5 0" className="hatchl" />
        </pattern>
      </defs>
      {(gaps ?? []).map((g, i) => {
        const gx = Math.max(0, x(g.from));
        const gw = Math.max(0, Math.min(width, x(g.to)) - gx);
        return gw < 1 ? null : (
          <rect
            key={`g${i}`}
            className={`gap ${g.kind}`}
            x={gx}
            y={0}
            width={gw}
            height={height}
            fill={g.kind === "off" ? `url(#${hatch})` : undefined}
          />
        );
      })}
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
  /// A moment picked: a point on the line, or (`atPeak`) the peak's dot.
  onPick?: (p: Pt, atPeak?: boolean) => void;
  /// Draw the y-axis labels (top and middle) at the left.
  axis?: boolean;
  /// Each point's peak (the busiest reading inside a minute), drawn as a faint band behind the
  /// line of averages, so a short spike, the peak dot and the tidemark have something to sit on.
  hi?: Pt[];
  /// The recorded sleeps: with `domain`, the chart shades its own gaps (where its line has no
  /// readings) as asleep or not recording, so a band always meets its line.
  sleeps?: Sleep[];
  /// Amber or red when the status is about this measure (status.ts); the tide colour otherwise.
  tone?: Tone;
  /// The time span across the chart (unix seconds): the range asked for, so a reading sits at
  /// its real time and the labels under the chart are true even when the record is shorter.
  domain?: [number, number];
  /// Thinner lines over the main one, each with its own dash: the parts of the whole (the
  /// performance and the efficiency cores of the CPU). The legend keys them by the same dash.
  also?: { pts: Pt[]; label: string; dash: string }[];
  /// The moment picked elsewhere on the screen (unix seconds), marked across the chart.
  marker?: number;
  /// What the marker's chip says; the time as the chart's own times read without it.
  markerLabel?: string;
}

/// A line over its area, with a crosshair that follows the pointer, snaps to the nearest
/// reading and shows it; the tidemark; and the peak marked with a dot and its value.
export function Area({
  pts: ptsIn,
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
  hi: bandIn,
  sleeps,
  tone,
  also: alsoIn,
  marker,
  markerLabel,
}: AreaProps) {
  const pts = useSame(ptsIn, samePts);
  const band = useSame(bandIn, samePts);
  const also = useSame(alsoIn, sameAlso);
  const color = toneColor(tone);
  const hatch = useId();
  const [d0, d1] = domain ?? [null, null];
  const gaps = useMemo(
    () =>
      sleeps && d0 != null && d1 != null
        ? gapsOf(
            pts.map((p) => p.t),
            [d0, d1],
            sleeps,
          )
        : [],
    [pts, sleeps, d0, d1],
  );
  const [ref, W] = useWidth<HTMLDivElement>();
  // The reading pointed at, by its time: a poll that adds a reading and drops the oldest leaves
  // the readout on the same moment, not on whatever now sits at the same index.
  const [hoverT, setHoverT] = useState<number | null>(null);
  // Pointing inside a gap shows the gap, not the nearest reading: the gap and where the pointer is.
  const [gapHover, setGapHover] = useState<{ g: Gap; px: number } | null>(null);
  // The readout sits above the chart, or below it where the screen's top bar would cover it.
  const tip = useRef<HTMLDivElement>(null);
  const [tipBelow, setTipBelow] = useState(false);
  useLayoutEffect(() => {
    const el = tip.current;
    const plot = ref.current;
    if (!el || !plot) return;
    // The screen's top bar, and Recent's tab strip pinned under it.
    const roof = Math.max(
      0,
      ...[...document.querySelectorAll(".topbar, .ptabs")].map(
        (e) => e.getBoundingClientRect().bottom,
      ),
    );
    setTipBelow(plot.getBoundingClientRect().top - 6 - el.offsetHeight < roof + 4);
    // Measured again for each reading pointed at: the readout's height follows its lines.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hoverT, gapHover]);
  const H = height;
  const left = axis ? 44 : 0;
  const w = Math.max(0, W - left);
  const hi =
    Math.max(max, mark?.v ?? 0, ...pts.map((p) => p.v), ...(band ?? []).map((p) => p.v)) || 1;
  const lo = min;
  const t0 = domain?.[0] ?? (pts.length ? pts[0].t : 0);
  const t1 = domain?.[1] ?? (pts.length ? pts[pts.length - 1].t : 1);
  const x = (t: number) => left + ((t - t0) / (t1 - t0 || 1)) * w;
  // Headroom above the scale where there is a peak to label, so its label always fits above
  // its dot instead of falling onto the line.
  const room = peak ? 26 : 0;
  const y = (v: number) => H - 1 - ((v - lo) / (hi - lo || 1)) * (H - 4 - room);
  const paths = useMemo(() => {
    return runs(pts, gapMin(step(pts))).map((r) => {
      const line = "M" + r.map((p) => `${x(p.t).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" L");
      return {
        line,
        area: `${line} L${x(r[r.length - 1].t).toFixed(1)} ${H} L${x(r[0].t).toFixed(1)} ${H} Z`,
        single: r.length === 1 ? r[0] : null,
      };
    });
    // x and y follow W, pts, the time span and the scale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pts, W, hi, lo, H, t0, t1, room]);
  const bandPaths = useMemo(() => {
    if (!band?.length) return [];
    return runs(band, gapMin(step(band))).map((r) => {
      const line = "M" + r.map((p) => `${x(p.t).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" L");
      return `${line} L${x(r[r.length - 1].t).toFixed(1)} ${H} L${x(r[0].t).toFixed(1)} ${H} Z`;
    });
    // x and y follow W, the band, the time span and the scale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [band, W, hi, lo, H, t0, t1, room]);
  const alsoPaths = useMemo(
    () =>
      (also ?? []).map((a) => ({
        dash: a.dash,
        lines: runs(a.pts, gapMin(step(a.pts))).map(
          (r) => "M" + r.map((p) => `${x(p.t).toFixed(1)} ${y(p.v).toFixed(1)}`).join(" L"),
        ),
      })),
    // x and y follow W, the lines, the time span and the scale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [also, W, hi, lo, H, t0, t1, room],
  );
  // The index of the reading nearest a time.
  const nearest = (t: number) => {
    if (!pts.length) return null;
    let best = 0;
    for (let i = 1; i < pts.length; i++)
      if (Math.abs(pts[i].t - t) < Math.abs(pts[best].t - t)) best = i;
    return best;
  };
  const hover = hoverT != null ? nearest(hoverT) : null;
  const hp = hover != null ? pts[hover] : null;
  // Now: a dot on the latest reading, where the chart runs up to the present and its line
  // reaches it (not across a pause or a sleep).
  const last = pts[pts.length - 1];
  // Within five minutes, or a reading's step on a chart recorded less often (hourly disk space).
  const nowPt =
    domain && last && domain[1] - last.t <= Math.max(300, 1.5 * step(pts)) ? last : null;
  // The moment picked, where it falls inside the chart, and where its chip sits.
  const pickAt = marker != null && marker >= t0 && marker <= t1 && W > 0 ? marker : null;
  const chipLeft = pickAt != null ? Math.min(Math.max(x(pickAt), left + 50), W - 50) : 0;
  const tipLeft = hp ? Math.min(Math.max(x(hp.t), left + 70), W - 70) : 0;
  // A peak at the top of the chart has its label below the dot, inside the card.
  return (
    <div className="area">
      <div
        className="plot"
        ref={ref}
        style={{ height: H }}
        onPointerMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const px = e.clientX - r.left;
          const t = t0 + ((px - left) / (w || 1)) * (t1 - t0);
          const g = (gaps ?? []).find((g) => t >= g.from && t <= g.to);
          if (g) {
            setGapHover({ g, px });
            setHoverT(null);
          } else {
            setGapHover(null);
            const i = nearest(t);
            setHoverT(i != null ? pts[i].t : null);
          }
        }}
        onPointerLeave={() => {
          setHoverT(null);
          setGapHover(null);
        }}
        onClick={() => {
          if (hp && onPick) onPick(hp);
        }}
      >
        {W > 0 && (
          <svg width={W} height={H} role="img" aria-label={label}>
            <line
              className="gridl"
              x1={left}
              x2={W}
              y1={room ? y(hi) : 0.5}
              y2={room ? y(hi) : 0.5}
            />
            <line className="gridl" x1={left} x2={W} y1={y((hi + lo) / 2)} y2={y((hi + lo) / 2)} />
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
              // Unlabelled: the legend keys the shading, and pointing at a gap says what it was.
              return (
                <rect
                  key={`g${i}`}
                  className={`gap ${g.kind}`}
                  x={gx}
                  y={0}
                  width={gw}
                  height={H}
                  fill={g.kind === "off" ? `url(#${hatch})` : undefined}
                />
              );
            })}
            {bandPaths.map((d, i) => (
              <path key={`b${i}`} d={d} fill={color} fillOpacity={0.1} />
            ))}
            {paths.map((p, i) =>
              p.single ? (
                <circle key={i} cx={x(p.single.t)} cy={y(p.single.v)} r={2} fill={color} />
              ) : (
                <g key={i}>
                  <path d={p.area} fill={color} fillOpacity={0.12} />
                  <path
                    d={p.line}
                    fill="none"
                    stroke={color}
                    strokeWidth={2}
                    strokeLinejoin="round"
                  />
                </g>
              ),
            )}
            {alsoPaths.map((a, i) =>
              a.lines.map((d, j) => (
                <path
                  key={`a${i}-${j}`}
                  d={d}
                  fill="none"
                  stroke={color}
                  strokeWidth={1.25}
                  strokeOpacity={0.8}
                  strokeDasharray={a.dash}
                  strokeLinejoin="round"
                />
              )),
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
            {pickAt != null && (
              <line className="pickl" x1={x(pickAt)} x2={x(pickAt)} y1={PICK_TOP} y2={H} />
            )}
            {hp && <line className="cross" x1={x(hp.t)} x2={x(hp.t)} y1={0} y2={H} />}
          </svg>
        )}
        {axis && W > 0 && (
          <>
            <span className="ax" style={{ top: room ? y(hi) : 0 }}>
              {fmt(hi)}
            </span>
            <span className="ax" style={{ top: y((hi + lo) / 2) }}>
              {fmt((hi + lo) / 2)}
            </span>
          </>
        )}
        {mark && mark.v > 0 && W > 0 && (
          // Above the line, where the chart is clear (the tidemark is the week's high); below it
          // only when the line is too near the top for the label to fit.
          <span
            className="tmlabel"
            style={{
              top: y(mark.v) >= 18 ? y(mark.v) - 3 : y(mark.v) + 3,
              transform: y(mark.v) >= 18 ? "translateY(-100%)" : undefined,
              // At the other end from the range's peak, so the two labels never meet.
              ...(peak && x(peak.t) < (left + W) / 2 ? { right: 4 } : { left: left + 4 }),
            }}
          >
            {mark.label}
          </span>
        )}
        {peak && W > 0 && (
          <>
            {onPick ? (
              // The peak as a target of its own: easier to hit than its point on the line.
              <button
                className="dot peakdot pick"
                style={{ left: x(peak.t), top: y(peak.v), "--c": color } as CSSProperties}
                aria-label={`Show the peak: ${peak.label}`}
                title="Show what the busiest apps were doing at the peak"
                onClick={(e) => {
                  e.stopPropagation();
                  onPick({ t: peak.t, v: peak.v }, true);
                }}
              />
            ) : (
              <span
                className="dot peakdot"
                style={{ left: x(peak.t), top: y(peak.v), background: color }}
              />
            )}
            {!(pickAt != null && Math.abs(chipLeft - x(peak.t)) < 110) && (
              <span
                className="peaklabel"
                style={{
                  left: Math.min(Math.max(x(peak.t), left + 50), W - 50),
                  top: y(peak.v),
                }}
              >
                {peak.label}
              </span>
            )}
          </>
        )}
        {pickAt != null && (
          // A chip saying when, its arrow pointing down the line to the moment.
          <span className="pickchip" style={{ left: chipLeft }}>
            {markerLabel ?? (fmtT ?? fmtAt)(pickAt)}
            <i style={{ left: `calc(50% + ${x(pickAt) - chipLeft}px)` }} />
          </span>
        )}
        {nowPt && W > 0 && (
          <span
            className="dot nowdot"
            style={{ left: x(nowPt.t), top: y(nowPt.v), "--c": color } as CSSProperties}
          />
        )}
        {gapHover && (
          <div
            ref={tip}
            className={`readout${tipBelow ? " below" : ""}`}
            style={{ left: Math.min(Math.max(gapHover.px, left + 80), W - 80) }}
          >
            <b>{gapHover.g.kind === "sleep" ? "Asleep" : "Not recording"}</b>
            <span>
              {fmtAt(gapHover.g.from)} to {fmtAt(gapHover.g.to)} ·{" "}
              {fmtMins(Math.max(1, Math.round((gapHover.g.to - gapHover.g.from) / 60)))}
            </span>
            {gapHover.g.kind === "off" && <span>Tidemark was quit or not yet started</span>}
          </div>
        )}
        {hp && (
          <>
            <span className="dot hoverdot" style={{ left: x(hp.t), top: y(hp.v) }} />
            <div
              ref={tip}
              className={`readout${tipBelow ? " below" : ""}`}
              style={{ left: tipLeft }}
            >
              <b>{fmt(hp.v)}</b>
              <span>
                {name} · {fmtT(hp.t)}
              </span>
              {hover != null && extra?.(hover)}
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
  // To a screen reader the cells are one picture, read as its busiest hour and weekday.
  let peak: { d: number; h: number; v: number } | null = null;
  for (const [d, row] of grid.entries())
    for (const [h, c] of row.entries())
      if (c != null && (!peak || c > peak.v)) peak = { d, h, v: c };
  const dayAvg = grid.map((row) => {
    const xs = row.filter((c): c is number => c != null);
    return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null;
  });
  const busyDay = dayAvg.reduce<number | null>(
    (b, a, d) => (a != null && (b == null || a > (dayAvg[b] ?? 0)) ? d : b),
    null,
  );
  const summary = peak
    ? `Busiest hour ${days[peak.d]} ${String(peak.h).padStart(2, "0")}:00, ${fmt(peak.v)}${busyDay != null ? `; busiest day on average ${days[busyDay]}` : ""}`
    : "Nothing recorded";
  return (
    <div className="heat" onPointerLeave={() => setTip(null)}>
      <div
        role="img"
        aria-label={`By weekday and hour: ${summary}`}
        style={{ display: "flex", flexDirection: "column", gap: 3 }}
      >
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
                />
              ))}
            </div>
          </div>
        ))}
      </div>
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
  also,
}: {
  line: string;
  band?: string;
  mark?: string;
  /// The chart's thinner lines (Area's `also`), each keyed by its dash.
  also?: { label: string; dash: string }[];
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
      {also?.map((a) => (
        <span className="lk" key={a.label}>
          <svg width="18" height="6" aria-hidden="true">
            <line className="lkline thin" x1="0" x2="18" y1="3" y2="3" strokeDasharray={a.dash} />
          </svg>
          {a.label}
        </span>
      ))}
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

/// How full a measure is right now, as a row of lit segments. `level` is 0 to 1; null shows
/// the row unlit.
export function Meter({
  level,
  tone,
  label,
}: {
  level: number | null;
  tone?: Tone;
  label: string;
}) {
  // Every segment the same whole number of pixels, so the edges stay sharp and the segments
  // equal: as many as fit, with the row centred in what is left over.
  const SEG = 3;
  const GAP = 3;
  const [ref, W] = useWidth<HTMLDivElement>();
  const N = Math.max(1, Math.floor((W + GAP) / (SEG + GAP)));
  const pad = Math.floor((W - (N * (SEG + GAP) - GAP)) / 2);
  const v = level == null ? 0 : Math.min(1, Math.max(0, level));
  // Any reading above nothing lights at least one segment.
  const lit = v > 0 ? Math.max(1, Math.round(v * N)) : 0;
  return (
    <div
      ref={ref}
      className="meter"
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v * 100)}
      style={{ "--c": toneColor(tone) } as CSSProperties}
    >
      {W > 0 &&
        Array.from({ length: N }, (_, i) => (
          <i
            key={i}
            className={i < lit ? "on" : undefined}
            style={{ left: pad + i * (SEG + GAP), width: SEG }}
          />
        ))}
    </div>
  );
}

/// Each core's use now, as a column of lit segments: performance cores, then efficiency cores,
/// where the Mac says which is which.
export function Cores({ use, kinds, tone }: { use: number[]; kinds: string; tone?: Tone }) {
  const SEG = 8;
  const all = use.map((v, i) => ({ v, i, k: kinds[i] ?? "" }));
  // Grouped by kind where every core is a P or an E; one group otherwise, so none is dropped.
  const groups =
    [...kinds].length === use.length && /^[PE]+$/.test(kinds)
      ? [
          { name: "performance", cores: all.filter((c) => c.k === "P") },
          { name: "efficiency", cores: all.filter((c) => c.k === "E") },
        ].filter((g) => g.cores.length)
      : [{ name: "", cores: all }];
  return (
    <div className="cores" style={{ "--c": toneColor(tone) } as CSSProperties}>
      {groups.map((g) => (
        <div className="grp" key={g.name} style={{ flexGrow: g.cores.length }}>
          <div className="cols">
            {g.cores.map((c) => {
              const lit = c.v > 0 ? Math.max(1, Math.round((Math.min(100, c.v) / 100) * SEG)) : 0;
              return (
                <div
                  className="col"
                  key={c.i}
                  title={`Core ${c.i + 1}${g.name ? ` (${g.name})` : ""}: ${Math.round(c.v)}%`}
                >
                  {Array.from({ length: SEG }, (_, j) => (
                    <i key={j} className={j < lit ? "on" : undefined} />
                  ))}
                </div>
              );
            })}
          </div>
          <span className="muted">
            {g.name ? `${g.cores.length} ${g.name}` : `${g.cores.length} cores`}
          </span>
        </div>
      ))}
    </div>
  );
}

/// The heading over the charts of other measures a tab shows for comparison: why they're there.
export function Alongside({ why }: { why: string }) {
  return (
    <div className="alongside">
      <span className="lbl">For comparison</span>
      <span>{why}</span>
    </div>
  );
}

/// The snapshot card beside the charts: pinned while they scroll past when it fits in the page's
/// view (under the tab strip, above the page's bottom padding), and scrolling with the page when it
/// is taller, rather than scrolling on its own inside a scrolling page.
export function Snap({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLElement>(null);
  const [tall, setTall] = useState(false);
  useEffect(() => {
    const el = ref.current;
    const page = el?.closest(".page");
    if (!el || !page) return;
    const check = () => setTall(el.offsetHeight > page.clientHeight - 62 - 28);
    const ro = new ResizeObserver(check);
    ro.observe(el);
    ro.observe(page);
    return () => ro.disconnect();
  }, []);
  return (
    <aside ref={ref} className={tall ? "card snap tall" : "card snap"}>
      {children}
    </aside>
  );
}
