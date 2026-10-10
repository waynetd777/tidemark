// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// The measures Tidemark shows, for picking each label's icon (icons.tsx).

export type Metric =
  | "cpu"
  | "memory"
  | "network"
  | "download"
  | "upload"
  | "swap"
  | "disk"
  | "diskio"
  | "thermal"
  | "energy"
  | "battery";

/// The icon for a measure's label ("CPU", "Disk I/O", "Memory high", "Disk used", ...).
export function metricOf(label: string): Metric {
  const l = label.toLowerCase();
  if (l.startsWith("cpu")) return "cpu";
  if (l.startsWith("memory")) return "memory";
  if (l.startsWith("network")) return "network";
  if (l.startsWith("download")) return "download";
  if (l.startsWith("upload")) return "upload";
  if (l.startsWith("swap")) return "swap";
  if (l.startsWith("disk i/o")) return "diskio";
  if (l.startsWith("thermal")) return "thermal";
  if (l.startsWith("energy") || l.startsWith("power")) return "energy";
  if (l.startsWith("battery")) return "battery";
  return "disk";
}
