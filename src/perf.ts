// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Hooks the screens share: the live readings and anything else fetched on a timer.

import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { Pt } from "./charts";
import type { Live, Sleep } from "./types";

/// Calls `load` now and every `ms`, keeping the last answer. A failed call keeps what was there.
/// `deps` restart it (a new range, a new root). The answer is kept with the deps it was asked
/// for, so `fresh` says whether it belongs to the current ones: null until the first answer.
export function usePolled<T>(
  load: () => Promise<T>,
  ms: number,
  deps: unknown[] = [],
): { v: T; fresh: boolean } | null {
  const key = JSON.stringify(deps);
  const [v, setV] = useState<{ key: string; v: T } | null>(null);
  const fn = useRef(load);
  useEffect(() => {
    fn.current = load;
  });
  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    // A slow answer must not overwrite a newer one.
    let asked = 0;
    let shown = 0;
    const tick = () => {
      window.clearTimeout(timer);
      // Behind other windows (or hidden), at most once a minute: nothing on screen to redraw,
      // and redrawing charts, or even waking to check, every few seconds costs energy. Back in
      // front, at once.
      timer = window.setTimeout(tick, document.hasFocus() ? ms : Math.max(ms, 60_000));
      const n = ++asked;
      fn.current()
        .then((r) => {
          if (alive && n > shown) {
            shown = n;
            setV({ key, v: r });
          }
        })
        .catch(() => {});
    };
    tick();
    window.addEventListener("focus", tick);
    return () => {
      alive = false;
      window.clearTimeout(timer);
      window.removeEventListener("focus", tick);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ms, ...deps]);
  return v && { v: v.v, fresh: v.key === key };
}

/// `usePolled`'s answer. A list keeps the old deps' answer until the new one comes, so it
/// doesn't blank each time they move; with `clear`, it's null until then instead, for a chart,
/// which would otherwise draw the old range on the new one's axis.
export function usePoll<T>(
  load: () => Promise<T>,
  ms: number,
  deps: unknown[] = [],
  { clear = false }: { clear?: boolean } = {},
): T | null {
  const p = usePolled(load, ms, deps);
  return p && (p.fresh || !clear) ? p.v : null;
}

/// The live readings, every 2 s.
export function useLive(): Live | null {
  return usePoll(api.live, 2000);
}

/// Start of today, unix seconds.
export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

export const now = () => Math.floor(Date.now() / 1000);

/// A series of finished minutes, ending at the latest live reading (unix seconds), so the chart
/// reaches now rather than stopping at the last whole minute or bucket.
export function upToNow(pts: Pt[], t: number | undefined, v: number | null | undefined): Pt[] {
  if (t == null || v == null || (pts.length && pts[pts.length - 1].t >= t)) return pts;
  return [...pts, { t, v }];
}

/// How long readings must stop, against their usual spacing, to be a gap: three steps, and
/// three minutes at least. The charts break their lines by the same rule, so a band and the end
/// of a line always meet.
export function gapMin(step: number): number {
  return Math.max(3 * step, 180);
}

/// A stretch of a chart with no readings: the Mac asleep, or Tidemark not recording (quit,
/// paused, or not yet started).
export interface Gap {
  from: number;
  to: number;
  kind: "sleep" | "off";
}

/// The gaps in a series of reading times (unix seconds) across `domain`: anywhere readings stop
/// for more than three of their usual steps (three minutes at least), and before the first. A gap
/// mostly covered by a recorded sleep is sleep.
export function gapsOf(ts: number[], domain: [number, number], sleeps: Sleep[]): Gap[] {
  const diffs = ts
    .slice(1)
    .map((t, i) => t - ts[i])
    .sort((a, b) => a - b);
  const step = diffs.length ? diffs[Math.floor(diffs.length / 2)] : 60;
  const min = gapMin(step);
  const raw: [number, number][] = [];
  if (!ts.length) raw.push(domain);
  else {
    if (ts[0] - domain[0] > min) raw.push([domain[0], ts[0]]);
    for (let i = 1; i < ts.length; i++) if (ts[i] - ts[i - 1] > min) raw.push([ts[i - 1], ts[i]]);
  }
  return raw.map(([from, to]) => {
    const asleep = sleeps.reduce(
      (s, sl) => s + Math.max(0, Math.min(to, sl.ended) - Math.max(from, sl.started)),
      0,
    );
    return { from, to, kind: asleep >= 0.5 * (to - from) ? "sleep" : "off" };
  });
}
