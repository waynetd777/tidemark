// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppInfo,
  AppUse,
  DiskTrend,
  FileEntry,
  GrowthInfo,
  Incident,
  Live,
  MacInfo,
  PerfHistory,
  Progress,
  Screen,
  Settings,
  Sleep,
  View,
} from "./types";

export interface Helper {
  available: boolean;
  on: boolean;
  needs_approval: boolean;
}

/// Listens for an event from the app; returns the function that stops listening.
export function on<T = unknown>(name: string, f: (payload: T) => void): () => void {
  const p = listen<T>(name, (e) => f(e.payload));
  return () => void p.then((u) => u());
}

export interface Cli {
  path: string | null;
  version: string | null;
  models: { id: string; name: string }[];
}
export interface AssistantStatus {
  claude: Cli;
  codex: Cli;
  antigravity: Cli;
  copilot: Cli;
}

export const api = {
  assistantStatus: () => invoke<AssistantStatus>("assistant_status"),
  /// Puts a question to Claude Code; the answer arrives as `ask-chunk` and `ask-done` events.
  ask: (chatId: string, prompt: string, model: string, session: string | null) =>
    invoke<void>("ask", { chatId, prompt, model, session }),
  askCancel: (chatId: string) => invoke<void>("ask_cancel", { chatId }),
  files: (path: string) => invoke<FileEntry[]>("get_files", { path }),
  state: () => invoke<AppInfo>("get_state"),
  progress: () => invoke<Progress>("get_progress"),
  /// The subtree under `path`, with folders smaller than `minFraction` of it folded away.
  tree: (path: string, minFraction = 1 / 4000) => invoke<View>("get_tree", { path, minFraction }),
  startScan: (root?: string) => invoke<void>("start_scan", { root: root ?? null }),
  stopScan: () => invoke<void>("stop_scan"),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  getInfo: (path: string) => invoke<void>("get_info", { path }),
  openPrivacySettings: () => invoke<void>("open_privacy_settings"),

  live: () => invoke<Live>("perf_live"),
  /// The last `secs` seconds of minutes, folded into at most `points` points.
  perf: (secs: number, points = 480, markDays = 7) =>
    invoke<PerfHistory>("perf_history", { secs, points, markDays }),
  appsBetween: (from: number, to: number) =>
    invoke<{ by_cpu: AppUse[]; by_mem: AppUse[]; by_energy: AppUse[] }>("perf_apps", { from, to }),
  sleeps: (days: number) => invoke<Sleep[]>("sleeps", { days }),
  incidents: (days: number) => invoke<Incident[]>("incidents", { days }),
  pauseRecording: (minutes: number) => invoke<number | null>("pause_recording", { minutes }),
  macInfo: () => invoke<MacInfo>("mac_info"),
  perfBrief: () => invoke<string>("perf_brief"),
  diskTrend: (days: number) => invoke<DiskTrend>("disk_trend", { days }),
  growth: (root: string, days: number) => invoke<GrowthInfo>("disk_growth", { root, days }),
  folderHistory: (path: string, days: number) =>
    invoke<[number, number][]>("folder_history", { path, days }),
  cpuByHour: () => invoke<(number | null)[][]>("cpu_by_hour"),
  historyInfo: () => invoke<{ bytes: number; since: number | null }>("history_info"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  showMain: (screen?: Screen) => invoke<void>("show_main_window", { screen: screen ?? null }),
  openAsk: () => invoke<void>("open_ask"),
  quit: () => invoke<void>("quit"),
  activityMonitor: () => invoke<void>("open_activity_monitor"),
  /// Open at Login: [available, on].
  loginItem: () => invoke<[boolean, boolean]>("login_item"),
  setLoginItem: (on: boolean) => invoke<boolean>("set_login_item", { on }),
  /// The background helper that records while the app is quit (recorder.rs).
  helper: () => invoke<Helper>("background_helper"),
  setHelper: (on: boolean) => invoke<Helper>("set_background_helper", { on }),
};
