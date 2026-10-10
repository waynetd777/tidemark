// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

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
}

export interface AppUse {
  app: string;
  cpu: number;
  mem: number;
}

export interface Status {
  level: "calm" | "attention";
  about: "" | "cpu" | "memory" | "swap" | "disk";
  headline: string;
  sub: string;
}

export interface Live {
  samples: Sample[];
  apps_cpu: AppUse[];
  apps_mem: AppUse[];
  processes: number;
  cores: number;
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
}

export interface Mark {
  value: number;
  at: number;
}

export interface PerfHistory {
  points: Minute[];
  tidemarks: Partial<Record<"cpu" | "mem" | "swap" | "disk" | "net", Mark>>;
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
