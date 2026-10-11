// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// What an Ask button beside an incident, a crash or a flagged chart tells the assistant, on top of
// the performance record every chat starts with: the thing itself, in full.

import type { AskDetail } from "./Ask";
import { api } from "./api";
import { incidentTitle } from "./incidents";
import { statusFor } from "./status";
import type { AppUse, Crash, Incident, Live } from "./types";
import { fmtAt, fmtMem, fmtW, pct } from "./util";

/// A moment in full, so the assistant can place it: "Wed 8 Oct 2026, 14:57".
const when = (ts: number) =>
  new Date(ts * 1000).toLocaleString("en-GB", {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

const appLine = (a: AppUse) =>
  `- ${a.app}: CPU ${pct(a.cpu)}, memory ${fmtMem(a.mem)}${a.energy ? `, ${fmtW(a.energy)}` : ""}`;

export function incidentQuestion(i: Incident): string {
  const what =
    i.kind === "cpu"
      ? "the CPU so busy"
      : i.kind === "pressure"
        ? "memory so tight"
        : "swap growing";
  return `Why was ${what} at ${fmtAt(i.started)}, and what can I do about it?`;
}

export function incidentDetail(i: Incident): AskDetail {
  const mins = i.ended != null ? Math.max(1, Math.round((i.ended - i.started) / 60)) : null;
  const peak = i.kind === "cpu" ? pct(i.peak) : i.kind === "swap" ? fmtMem(i.peak) : "red";
  const rule =
    i.kind === "cpu"
      ? "CPU above 90% for 30 seconds or more"
      : i.kind === "pressure"
        ? "macOS's memory pressure turned red"
        : "swap grew by 1 GB or more within 10 minutes";
  return {
    label: `${incidentTitle(i)}, ${fmtAt(i.started)}`,
    text: [
      `An incident Tidemark recorded: ${rule}.`,
      `Started ${when(i.started)}; ${mins == null ? "still going on" : `lasted ${mins} minute${mins === 1 ? "" : "s"}, ended ${when(i.ended!)}`}.`,
      `Peak ${peak} at ${when(i.peak_at)}.`,
      "What the busiest apps were using at the peak, busiest first (CPU is a share of the whole Mac):",
      ...i.snapshot.slice(0, 15).map(appLine),
    ].join("\n"),
  };
}

export function crashQuestion(g: { app: string; kind: Crash["kind"]; n: number }): string {
  return g.kind === "hang"
    ? `Why did ${g.app} stop responding, and how do I stop it happening again?`
    : `Why did ${g.app} crash${g.n > 1 ? ` ${g.n} times this week` : ""}, and how do I fix it?`;
}

/// The reports for one app's crashes or hangs this week, the latest three summarised by the
/// backend (exception, termination, the crashed thread's stack).
export async function crashDetail(
  g: { app: string; kind: Crash["kind"]; n: number; last: number },
  all: Crash[],
): Promise<AskDetail> {
  const mine = all.filter((c) => c.app === g.app && c.kind === g.kind).sort((a, b) => b.ts - a.ts);
  const reports = await Promise.all(
    mine
      .slice(0, 3)
      .map((c) => api.crashReport(c.file).catch((e) => `Report: ${c.file}\n(${String(e)})`)),
  );
  return {
    label: `${g.app} ${g.kind === "hang" ? "hang" : "crash"} report${g.n > 1 ? "s" : ""}, ${fmtAt(g.last)}`,
    text: [
      `macOS wrote ${g.n} ${g.kind === "hang" ? "hang" : "crash"} report${g.n > 1 ? "s" : ""} for ${g.app} this week, at: ${mine.map((c) => when(c.ts)).join("; ")}.`,
      `The latest ${reports.length === 1 ? "one" : reports.length}, summarised:`,
      ...reports.map((r) => `---\n${r}`),
    ].join("\n"),
  };
}

/// A chart flagged amber or red: what the status says, the chart's own figures and what each app
/// is using now, sorted by the measure the chart is about.
export function flaggedDetail(
  live: Live | null,
  measure: string,
  figures: string[],
): { question: string; detail: AskDetail } {
  const st = statusFor(live?.status, measure) ?? live?.status;
  const by: "cpu" | "mem" | "energy" =
    measure === "Memory" ? "mem" : measure === "Energy" ? "energy" : "cpu";
  const apps = [...(live?.apps_cpu ?? []), ...(live?.apps_mem ?? []), ...(live?.apps_energy ?? [])];
  const seen = new Set<string>();
  const top = apps
    .sort((a, b) => b[by] - a[by])
    .filter((a) => !seen.has(a.app) && !!seen.add(a.app))
    .slice(0, 12);
  const question =
    measure === "Disk space"
      ? "My disk is getting full. What is using the space, and what can I safely clear?"
      : `Why is ${measure === "CPU" ? "the CPU" : measure.toLowerCase()} ${st?.label ? st.label.toLowerCase() : "high"} right now, and what should I do?`;
  return {
    question,
    detail: {
      label: `${measure}: ${st?.label ?? "flagged"}`,
      text: [
        `Tidemark has flagged ${measure} as ${st?.severity === "critical" ? "critical" : "a warning"} (${st?.label ?? ""}), ${when(Date.now() / 1000)}.`,
        st ? `It says: ${st.headline}. ${st.sub}` : "",
        ...figures,
        measure === "Disk space"
          ? ""
          : `What each app is using now, by ${by === "mem" ? "memory" : by === "energy" ? "power" : "CPU"}:`,
        ...(measure === "Disk space" ? [] : top.map(appLine)),
      ]
        .filter(Boolean)
        .join("\n"),
    },
  };
}
