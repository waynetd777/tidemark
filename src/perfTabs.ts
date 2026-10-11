// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The page's tabs: every measure together, or one with what goes with it in more room.
export const TABS = [
  { id: "all", label: "All", m: null },
  { id: "cpu", label: "CPU", m: "cpu" },
  { id: "gpu", label: "GPU", m: "gpu" },
  { id: "memory", label: "Memory", m: "memory" },
  { id: "diskio", label: "Disk I/O", m: "diskio" },
  { id: "network", label: "Network", m: "network" },
  { id: "energy", label: "Energy", m: "energy" },
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
    }) as Record<string, Tab>
  )[target] ?? "all";
