// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// How an incident reads, wherever it is listed.

import type { Incident } from "./types";
import { fmt, pct } from "./util";

export function incidentTitle(i: Incident): string {
  if (i.kind === "cpu") return `CPU reached ${pct(i.peak)}`;
  if (i.kind === "pressure") return `Memory pressure went red`;
  return `Swap grew to ${fmt(i.peak)}`;
}

export function incidentWhy(i: Incident): string {
  const mins = i.ended != null ? Math.max(1, Math.round((i.ended - i.started) / 60)) : null;
  const top = i.snapshot
    .slice(0, 2)
    .map((a) => (i.kind === "cpu" ? `${a.app} (${pct(a.cpu)})` : `${a.app} (${fmt(a.mem)})`))
    .join(" and ");
  const span = mins == null ? "Still going on" : `${mins} minute${mins === 1 ? "" : "s"}`;
  return top ? `${span}: mostly ${top}.` : `${span}.`;
}
