// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// How an incident reads, wherever it is listed.

import type { Crash, Incident } from "./types";
import { fmtMem, pct } from "./util";

export function incidentTitle(i: Incident): string {
  if (i.kind === "cpu") return `CPU reached ${pct(i.peak)}`;
  if (i.kind === "pressure") return `Memory pressure went red`;
  return `Swap grew to ${fmtMem(i.peak)}`;
}

export function incidentWhy(i: Incident): string {
  const mins = i.ended != null ? Math.max(1, Math.round((i.ended - i.started) / 60)) : null;
  const top = i.snapshot
    .slice(0, 2)
    .map((a) => (i.kind === "cpu" ? `${a.app} (${pct(a.cpu)})` : `${a.app} (${fmtMem(a.mem)})`))
    .join(" and ");
  const span = mins == null ? "Still going on" : `${mins} minute${mins === 1 ? "" : "s"}`;
  return top ? `${span}: mostly ${top}.` : `${span}.`;
}

/// Crashes and hangs grouped by app and kind, most recent first: "Xcode crashed 3 times".
export function crashGroups(
  list: Crash[],
): { app: string; kind: Crash["kind"]; n: number; last: number }[] {
  const by = new Map<string, { app: string; kind: Crash["kind"]; n: number; last: number }>();
  for (const c of list) {
    const k = `${c.app}\u0000${c.kind}`;
    const e = by.get(k) ?? { app: c.app, kind: c.kind, n: 0, last: 0 };
    e.n += 1;
    e.last = Math.max(e.last, c.ts);
    by.set(k, e);
  }
  return [...by.values()].sort((a, b) => b.last - a.last);
}

export function crashTitle(g: { app: string; kind: Crash["kind"]; n: number }): string {
  const verb = g.kind === "hang" ? "hung" : "crashed";
  return `${g.app} ${verb}${g.n > 1 ? ` ${g.n} times` : ""}`;
}
