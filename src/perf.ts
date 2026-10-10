// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// Hooks the screens share: the live readings and anything else fetched on a timer.

import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { Pt } from "./charts";
import type { Live } from "./types";

/// Calls `load` now and every `ms`, keeping the last answer. A failed call keeps what was there.
/// `deps` restart it (a new range, a new root).
export function usePoll<T>(load: () => Promise<T>, ms: number, deps: unknown[] = []): T | null {
  const [v, setV] = useState<T | null>(null);
  const fn = useRef(load);
  useEffect(() => {
    fn.current = load;
  });
  useEffect(() => {
    let alive = true;
    const tick = () =>
      fn
        .current()
        .then((r) => {
          if (alive) setV(r);
        })
        .catch(() => {});
    void tick();
    const id = window.setInterval(tick, ms);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ms, ...deps]);
  return v;
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
