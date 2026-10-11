// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The page's tabs: every measure together, or one with what goes with it in more room.
export const TABS = [
  { id: "all", label: "All", m: null, tip: "Every measure, one chart each, side by side in time" },
  {
    id: "cpu",
    label: "CPU",
    m: "cpu",
    tip: "The CPU, its performance and efficiency cores, the apps using it and its incidents",
  },
  {
    id: "gpu",
    label: "GPU",
    m: "gpu",
    tip: "How busy the graphics processor was, beside the CPU and the power drawn",
  },
  {
    id: "memory",
    label: "Memory",
    m: "memory",
    tip: "Memory in use and what it's made of, swap, pressure, and the apps holding it",
  },
  {
    id: "diskio",
    label: "Disk I/O",
    m: "diskio",
    tip: "Reads and writes, swap, the busiest moments and the apps using the disk",
  },
  {
    id: "network",
    label: "Network",
    m: "network",
    tip: "Download and upload, and their busiest moments",
  },
  {
    id: "energy",
    label: "Energy",
    m: "energy",
    tip: "The power drawn, the battery, what draws it, and the apps using the most energy",
  },
  {
    id: "disk",
    label: "Disk space",
    m: "disk",
    tip: "Used and free space, what grew this week and what's using the space",
  },
] as const;
export type Tab = (typeof TABS)[number]["id"];
export const isTab = (v: unknown): v is Tab => TABS.some((t) => t.id === v);
/// The tab a Details link's target belongs on.
export const tabFor = (target: string): Tab =>
  (
    ({
      cpu: "cpu",
      gpu: "gpu",
      memory: "memory",
      swap: "memory",
      "disk i/o": "diskio",
      network: "network",
      energy: "energy",
      crashes: "cpu",
      "disk space": "disk",
    }) as Record<string, Tab>
  )[target] ?? "all";
