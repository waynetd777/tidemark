// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

export interface Volume {
  name: string;
  total: number;
  free: number;
  used: number;
}

export interface ScanSummary {
  finished: string;
  duration_ms: number;
  items: number;
  size: number;
  apparent: number;
  denied: number;
  cache_bytes: number;
}

export interface Recent {
  path: string;
  name: string;
  size: number;
  items: number;
  when: string;
}

export interface AppInfo {
  root: string;
  root_name: string;
  scanning: boolean;
  loading: boolean;
  scan: ScanSummary | null;
  volume: Volume;
  recent: Recent[];
  full_disk_access: boolean;
  home: string;
}

export interface Progress {
  scanning: boolean;
  items: number;
  bytes: number;
  dirs: number;
  denied: number;
  current: string;
  elapsed_ms: number;
  expected_items: number | null;
  /// What is being scanned: a scheduled scan of the disk can run behind another folder.
  root_name: string;
}

/// One folder as the treemap draws it. `depth` is from the scan root (root = 0).
export interface View {
  name: string;
  path: string;
  size: number;
  apparent: number;
  files: number;
  loose_size: number;
  loose_apparent: number;
  items: number;
  cloud: boolean;
  denied: number;
  depth: number;
  /// Layout weight: size on disk, but a cloud folder is never less than 2% of its cloud size.
  weight: number;
  kids: View[];
  more: number;
  more_size: number;
  more_apparent: number;
  more_weight: number;
}

export interface FileEntry {
  name: string;
  path: string;
  size: number;
  disk_size: number;
  modified: number | null;
  kind: string;
}

// ---- performance and history ----

/// One reading of the Mac, every 2-5 s. Bytes, bytes a second, CPU as % of the whole machine.
export interface Sample {
  /// Unix milliseconds (a chart's Pt.t is seconds).
  t: number;
  cpu: number;
  mem: number;
  swap: number;
  /// macOS memory pressure: 1 normal, 2 warning, 4 critical.
  pressure: number;
  disk_r: number;
  disk_w: number;
  net_in: number;
  net_out: number;
  /// 0 nominal, 1 fair, 2 serious, 3 critical.
  thermal: number;
  /// What the Mac draws, watts; null without a battery controller.
  power: number | null;
  /// Battery charge, 0-100; null without a battery.
  battery: number | null;
  /// GPU utilisation, 0-100; null if no GPU reports it.
  gpu: number | null;
  /// What memory in use is made of, bytes.
  mem_parts: MemParts | null;
  on_ac: boolean;
  /// The performance and the efficiency cores' average use, 0-100; null where the Mac doesn't
  /// say which core is which.
  cpu_p: number | null;
  cpu_e: number | null;
}

export interface MemParts {
  app: number;
  wired: number;
  compressed: number;
}

export interface Crash {
  ts: number;
  app: string;
  kind: "crash" | "hang";
  /// The report macOS wrote.
  file: string;
}

export interface Battery {
  percent: number;
  charging: boolean;
  on_ac: boolean;
  fully_charged: boolean;
  watts: number | null;
  minutes_left: number | null;
  cycles: number | null;
  /// Capacity now as a share of the design capacity, 0-100.
  health: number | null;
}

export interface Sleep {
  started: number;
  ended: number;
  battery_before: number | null;
  battery_after: number | null;
}

export interface AppUse {
  app: string;
  cpu: number;
  mem: number;
  /// Watts.
  energy: number;
}

export interface Status {
  level: "calm" | "attention";
  about: "" | "cpu" | "memory" | "swap" | "disk" | "energy";
  /// "warn", "critical", or "" when calm.
  severity: "" | "warn" | "critical";
  /// A word or two for the measure's card: "Getting tight", "Critical".
  label: string;
  headline: string;
  sub: string;
  /// The other measures needing attention, worst first: each colours its own card.
  also: Status[];
}

export interface Live {
  samples: Sample[];
  apps_cpu: AppUse[];
  apps_mem: AppUse[];
  apps_energy: AppUse[];
  battery: Battery | null;
  processes: number;
  cores: number;
  /// Each core's use now, 0-100, and its kind, "P" or "E" per core ("" where the Mac doesn't say).
  core_use: number[];
  core_kinds: string;
  mem_total: number;
  paused_until: number | null;
  status: Status;
}

/// One minute of history, or a bucket of minutes: averages and maxima.
export interface Minute {
  ts: number;
  cpu: number;
  cpu_max: number;
  mem: number;
  mem_max: number;
  mem_total: number;
  swap: number;
  swap_max: number;
  pressure: number;
  disk_r: number;
  disk_w: number;
  disk_max: number;
  net_in: number;
  net_out: number;
  net_max: number;
  thermal: number;
  /// Minutes recorded in this point, so rate × minutes × 60 is what moved.
  minutes: number;
  power: number | null;
  power_max: number | null;
  battery: number | null;
  on_ac: boolean;
  gpu: number | null;
  gpu_max: number | null;
  mem_app: number | null;
  mem_wired: number | null;
  mem_comp: number | null;
  /// The busiest moment of download and of upload, bytes a second.
  net_in_max: number;
  net_out_max: number;
  /// The performance and the efficiency cores' average use; null before they were recorded.
  cpu_p: number | null;
  cpu_e: number | null;
}

export interface Mark {
  value: number;
  at: number;
}

export interface PerfHistory {
  points: Minute[];
  /// The highest of each over the span asked for. "disk" is disk I/O (reads and writes);
  /// "down" and "up" are download and upload apart, "net" both together.
  tidemarks: Partial<
    Record<"cpu" | "mem" | "swap" | "disk" | "net" | "power" | "gpu" | "down" | "up", Mark>
  >;
  since: number | null;
}

export interface Incident {
  id: number;
  kind: "cpu" | "pressure" | "swap";
  started: number;
  ended: number | null;
  peak: number;
  peak_at: number;
  snapshot: AppUse[];
}

export interface DiskPoint {
  ts: number;
  used: number;
  total: number;
}

export interface DiskTrend {
  points: DiskPoint[];
  high: DiskPoint | null;
}

export interface Growth {
  path: string;
  size: number;
  before: number;
  delta: number;
}

export interface GrowthInfo {
  landed: Growth[];
  /// What `landed` adds up to, a folder inside another listed one counted once.
  total: number;
  all: Growth[];
  from: number | null;
  to: number | null;
}

export interface Settings {
  history_days: number;
  scan: "daily" | "hourly" | "off";
  theme: "system" | "light" | "dark";
}

export interface MacInfo {
  chip: string;
  cores: number;
  memory: number;
  uptime_secs: number;
}

export type Screen = "now" | "disk" | "performance" | "trends" | "settings";
