// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The menu-bar window: how the Mac is doing in a sentence, CPU, memory and disk with their
// sparklines and today's highs, what is using the most, the week's disk growth, and the few
// things worth doing from the menu bar. Built the way Keepr's is: it re-reads everything each
// time it opens (System Settings can change Open at Login behind its back), every item closes it,
// the shortcuts it shows work while it is open, and it sizes its window to what it shows.

import { useCallback, useEffect, useRef, useState } from "react";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Labelled } from "./icons";
import { metricOf } from "./metrics";
import { api, on } from "./api";
import { Meter, Spark, type Pt } from "./charts";
import { Mark } from "./Mark";
import { flagFor } from "./status";
import Tooltips from "./Tooltips";
import { startOfToday, upToNow } from "./perf";
import type { DiskTrend, GrowthInfo, Live, Minute, PerfHistory, Sleep } from "./types";
import { abbrev, batteryLine, fmt, fmtAt, fmtDelta, fmtMem, fmtRate, fmtW, pct } from "./util";

const WIDTH = 360;
const hide = () => void getCurrentWindow().hide();

function Item({
  label,
  keys,
  onClick,
  strong,
  tip,
}: {
  label: string;
  keys?: string;
  onClick: () => unknown;
  strong?: boolean;
  tip: string;
}) {
  return (
    <button
      className={`menu-item${strong ? " strong" : ""}`}
      title={tip}
      onClick={() => {
        void onClick();
        hide();
      }}
    >
      <span className="grow">{label}</span>
      {keys && <span className="k">{keys}</span>}
    </button>
  );
}

export function Switch({
  on,
  label,
  onChange,
  disabled,
}: {
  on: boolean;
  label: string;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      className={`switch${on ? " on" : ""}`}
      disabled={disabled}
      title={`${label}: ${on ? "on" : "off"}`}
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => onChange(!on)}
    >
      <span />
    </button>
  );
}

export default function Tray() {
  const [live, setLive] = useState<Live | null>(null);
  const [day, setDay] = useState<PerfHistory | null>(null);
  // The lines: the last hour minute by minute, and the disk's last day, as on Now.
  const [hour, setHour] = useState<PerfHistory | null>(null);
  const [diskDay, setDiskDay] = useState<DiskTrend | null>(null);
  const [disk, setDisk] = useState<DiskTrend | null>(null);
  const [growth, setGrowth] = useState<GrowthInfo | null>(null);
  const [home, setHome] = useState("");
  const [helperOn, setHelperOn] = useState(false);
  const [sleeps, setSleeps] = useState<Sleep[]>([]);
  // The window is created hidden; "tray-opened" opens it. Closed, it reads nothing live.
  const [open, setOpen] = useState(false);

  // Everything but the live readings, read again each time the menu opens.
  const load = useCallback(() => {
    api
      .perf(86400, 96)
      .then(setDay)
      .catch(() => {});
    api
      .diskTrend(7)
      .then(setDisk)
      .catch(() => {});
    api
      .perf(3600, 60)
      .then(setHour)
      .catch(() => {});
    api
      .diskTrend(1)
      .then(setDiskDay)
      .catch(() => {});
    api
      .growth("/", 7)
      .then(setGrowth)
      .catch(() => {});
    api
      .state()
      .then((s) => setHome(s.home))
      .catch(() => {});
    api
      .sleeps(1)
      .then(setSleeps)
      .catch(() => {});
    api
      .helper()
      .then((h) => setHelperOn(h.on))
      .catch(() => {});
  }, []);
  useEffect(() => {
    load();
    // A screenshot of the menu-bar window shows it open.
    void api
      .scene()
      .then((sc) => {
        if (sc?.tray) setOpen(true);
      })
      .catch(() => {});
    const u1 = on("tray-opened", () => {
      setOpen(true);
      load();
    });
    const u2 = on("scan-finished", load);
    const w = getCurrentWindow();
    const u3 = w.onFocusChanged(({ payload }) => {
      if (!payload) setOpen(false);
    });
    return () => {
      u1();
      u2();
      void u3.then((u) => u());
    };
  }, [load]);
  // The live readings, only while the menu is open.
  useEffect(() => {
    if (!open) return;
    let alive = true;
    const tick = () =>
      api
        .live()
        .then((l) => alive && setLive(l))
        .catch(() => {});
    void tick();
    const id = window.setInterval(tick, 2000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [open]);

  const attention = live?.status.level === "attention";
  const about = live?.status.about ?? "";
  // The one action worth offering from here: what needs attention, if anything does.
  const primary = !attention
    ? null
    : about === "disk"
      ? {
          label: "Show the disk map",
          keys: "⌘4",
          run: () => api.showMain("disk"),
          tip: "Open the map to see what is filling the disk",
        }
      : {
          label: `Show what's using ${about === "cpu" ? "the CPU" : about === "energy" ? "energy" : "memory"}`,
          keys: "⌘2",
          run: () => api.showMain("performance"),
          tip: "Open Recent to see which apps are responsible",
        };

  // The shortcuts the menu shows work while it is open.
  useEffect(() => {
    const keys: Record<string, () => unknown> = {
      o: () => api.showMain(),
      ",": () => api.showMain("settings"),
      q: () => api.quit(),
      "2": () => api.showMain("performance"),
      "4": () => api.showMain("disk"),
    };
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape") return hide();
      if (!e.metaKey) return;
      const f = keys[e.key.toLowerCase()];
      if (f) {
        e.preventDefault();
        void f();
        hide();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);

  // Size the window to what it shows (observed again once there is something to show).
  const loaded = live !== null;
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      void getCurrentWindow().setSize(
        new LogicalSize(WIDTH, Math.ceil(el.getBoundingClientRect().height)),
      );
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [loaded]);

  if (!live) return <div className="traywin" ref={root} />;

  const latest = live.samples[live.samples.length - 1];
  // The lines cover the last hour, as on Now: the minutes, then the live reading.
  const liveDomain: [number, number] = latest ? [latest.t / 1000 - 3600, latest.t / 1000] : [0, 1];
  const lastHour = (pick: (m: Minute) => number | null, now: number | null | undefined): Pt[] =>
    upToNow(
      (hour?.points ?? [])
        .map((m) => ({ t: m.ts, v: pick(m) }))
        .filter((p): p is Pt => p.v != null),
      latest ? latest.t / 1000 : undefined,
      now,
    );
  const today = startOfToday();
  const highToday = (k: "cpu_max" | "mem_max") => {
    let best: { v: number; t: number } | null = null;
    for (const m of day?.points ?? [])
      if (m.ts >= today && (!best || m[k] > best.v)) best = { v: m[k], t: m.ts };
    return best;
  };
  const cpuHigh = highToday("cpu_max");
  const memHigh = highToday("mem_max");
  const tm = day?.tidemarks ?? {};
  const series = (k: "cpu" | "mem"): Pt[] => lastHour((m) => m[k], latest?.[k]);
  const diskPts: Pt[] = (disk?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const dayDiskPts: Pt[] = (diskDay?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const diskNow = diskPts[diskPts.length - 1]?.v ?? 0;
  const diskTotal = disk?.points[disk.points.length - 1]?.total ?? 0;
  const startOfDay = (disk?.points ?? []).find((p) => p.ts >= today);
  const diskToday = startOfDay ? diskNow - startOfDay.used : 0;
  const memWarn = about === "memory" || about === "swap";
  const memNewMark = memWarn && tm.mem && latest && latest.mem >= tm.mem.value * 0.98;

  // The busiest moment of the hour, for the rate meters (as on Now).
  const hourTop = (pick: (m: Minute) => number | null) =>
    Math.max(1, ...(hour?.points ?? []).map((m) => pick(m) ?? 0));
  const rows = [
    {
      name: "CPU",
      level: latest ? latest.cpu / 100 : null,
      note: cpuHigh ? `Today's high ${pct(cpuHigh.v)} · ${fmtAt(cpuHigh.t)}` : "Recording",
      flag: flagFor(live.status, "CPU"),
      value: latest ? pct(latest.cpu) : "–",
      spark: (
        <Spark
          sleeps={sleeps}
          domain={liveDomain}
          pts={series("cpu")}
          max={100}
          mark={tm.cpu?.value}
          tone={flagFor(live.status, "CPU")?.tone}
          label="CPU, last hour"
        />
      ),
    },
    {
      name: "Memory",
      level: latest && live.mem_total ? latest.mem / live.mem_total : null,
      note: memNewMark
        ? `New tidemark · swap ${fmtMem(latest.swap)}`
        : memHigh
          ? `Today's high ${fmtMem(memHigh.v)}`
          : "Recording",
      flag: flagFor(live.status, "Memory"),
      value: latest ? fmtMem(latest.mem) : "–",
      spark: (
        <Spark
          sleeps={sleeps}
          domain={liveDomain}
          pts={series("mem")}
          max={live.mem_total}
          mark={tm.mem?.value}
          tone={flagFor(live.status, "Memory")?.tone}
          label="Memory used, last hour"
        />
      ),
    },
    {
      name: "Network",
      level: latest ? latest.net_in / hourTop((m) => Math.max(m.net_in_max, m.net_in)) : null,
      note: latest ? `↑ ${fmtRate(latest.net_out)} up` : "Recording",
      flag: null,
      value: latest ? `↓ ${fmtRate(latest.net_in)}` : "–",
      spark: (
        <Spark
          sleeps={sleeps}
          domain={liveDomain}
          pts={lastHour((m) => m.net_in, latest?.net_in)}
          max={Math.max(1, ...lastHour((m) => m.net_in, latest?.net_in).map((p) => p.v))}
          label="Download, last hour"
        />
      ),
    },
    {
      name: "Energy",
      level:
        latest?.power != null
          ? latest.power /
            Math.max(
              tm.power?.value ?? 0,
              hourTop((m) => m.power_max ?? m.power),
            )
          : null,
      note: live.battery ? batteryLine(live.battery) : "No battery",
      flag: flagFor(live.status, "Energy"),
      value: latest?.power != null ? fmtW(latest.power) : "–",
      spark: (
        <Spark
          sleeps={sleeps}
          domain={liveDomain}
          pts={lastHour((m) => m.power, latest?.power)}
          max={Math.max(1, ...lastHour((m) => m.power, latest?.power).map((p) => p.v))}
          tone={flagFor(live.status, "Energy")?.tone}
          label="Power drawn, last hour"
        />
      ),
    },
    {
      name: "Disk space",
      level: diskTotal ? diskNow / diskTotal : null,
      note: diskToday > 1e8 ? `${fmtDelta(diskToday)} today` : "Steady today",
      flag: flagFor(live.status, "Disk space"),
      value: diskTotal ? `${fmt(diskTotal - diskNow)} free` : "–",
      spark: (
        <Spark
          pts={dayDiskPts}
          min={dayDiskPts.length ? Math.min(...dayDiskPts.map((p) => p.v)) * 0.995 : 0}
          max={Math.max(1, ...dayDiskPts.map((p) => p.v), disk?.high?.used ?? 0)}
          mark={disk?.high?.used}
          tone={flagFor(live.status, "Disk space")?.tone}
          label="Disk space used, last 24 hours"
        />
      ),
    },
  ];

  const top = (about === "cpu" ? live.apps_cpu : live.apps_mem).slice(0, 3);
  const landed = (growth?.landed ?? []).filter((g) => g.delta > 0);
  const grew = Math.max(0, growth?.total ?? 0);
  const memHog = live.apps_mem[0];
  // The week's row, only when there is something to say: a warning, or real disk growth.
  const week =
    memWarn && memHog && live.mem_total
      ? {
          warn: true,
          text: `${memHog.app} holds ${Math.round((100 * memHog.mem) / live.mem_total)}% of your memory`,
          note: "Quitting it when you're not using it frees the most.",
          go: () => api.showMain("performance"),
        }
      : grew >= 1e9
        ? {
            warn: false,
            text: `Disk grew ${fmt(grew)} this week`,
            note: landed[0]
              ? `Mostly ${abbrev(landed[0].path, home)} (${fmtDelta(landed[0].delta)}). Open the map to see it.`
              : "Open the map to see where.",
            go: () => api.showMain("disk"),
          }
        : null;

  return (
    <div className="traywin" ref={root}>
      <Tooltips />
      <div className="tsec thead">
        <div className="row">
          <Mark size={22} />
          <span className="theadline">{live.status.headline}</span>
        </div>
        <span className="muted ell">{live.status.sub}</span>
      </div>

      <div className="tsec pad8">
        {rows.map((r) => (
          <button
            key={r.name}
            className="menu-item tall"
            title={`Open Recent at ${r.name.toLowerCase()}`}
            onClick={() => {
              // Recent's card for the row: "network" for its download row.
              void api.showMain(
                "performance",
                r.name === "Network" ? "network" : r.name.toLowerCase(),
              );
              hide();
            }}
          >
            <span className="tname">
              <span className="nm">
                <Labelled m={metricOf(r.name)}>{r.name}</Labelled>
              </span>
              <Meter level={r.level} tone={r.flag?.tone} label={`${r.name} now`} />
              <span className={`tiny ${r.flag ? r.flag.cls : "muted"}`}>{r.note}</span>
            </span>
            {r.spark}
            <span className={`tval${r.flag ? ` ${r.flag.cls}` : ""}`}>{r.value}</span>
          </button>
        ))}
      </div>

      <div className="tsec tops">
        <div className="muted tiny tl">Using the most right now</div>
        {top.map((a) => (
          <button
            key={a.app}
            className="menu-item"
            title="Open Recent to see every app"
            onClick={() => {
              void api.showMain("performance");
              hide();
            }}
          >
            <span className="grow ell">{a.app}</span>
            <span className="tiny muted">
              {fmtMem(a.mem)} · {pct(a.cpu)} CPU
            </span>
          </button>
        ))}
      </div>

      {week && (
        <div className="tsec pad8">
          <button
            className="menu-item tall"
            title="Open the details"
            onClick={() => {
              void week.go();
              hide();
            }}
          >
            <span className={`sdot${week.warn ? " warn" : ""}`} />
            <span className="col grow minw0">
              <span className="ell">{week.text}</span>
              <span className="tiny muted ell">{week.note}</span>
            </span>
          </button>
        </div>
      )}

      <div className="tactions">
        {primary && (
          <>
            <Item
              label={primary.label}
              keys={primary.keys}
              strong
              onClick={primary.run}
              tip={primary.tip}
            />
            <div className="msep" />
          </>
        )}
        <Item
          label="Open Tidemark"
          keys="⌘O"
          onClick={() => api.showMain()}
          tip="Open the main window"
        />
        <Item
          label="Settings…"
          keys="⌘,"
          onClick={() => api.showMain("settings")}
          tip="History, disk scans, appearance and Open at Login"
        />
        <Item
          label="Quit Tidemark"
          keys="⌘Q"
          onClick={() => api.quit()}
          tip={
            helperOn
              ? "Quit. The background helper keeps recording"
              : "Quit. Nothing is recorded until Tidemark is opened again"
          }
        />
      </div>
      <div className="tfoot">
        {helperOn
          ? "Tidemark keeps recording even when it's quit."
          : "Tidemark keeps recording when the window is closed."}
      </div>
    </div>
  );
}
