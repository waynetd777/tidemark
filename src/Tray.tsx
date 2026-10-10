// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// The menu-bar window: how the Mac is doing in a sentence, CPU, memory and disk with their
// sparklines and today's highs, what is using the most, the week's disk growth, and the few
// things worth doing from the menu bar. Built the way Keepr's is: it re-reads everything each
// time it opens (System Settings can change Open at Login behind its back), every item closes it,
// the shortcuts it shows work while it is open, and it sizes its window to what it shows.

import { useCallback, useEffect, useRef, useState } from "react";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, on } from "./api";
import { Spark, type Pt } from "./charts";
import { Mark } from "./Mark";
import Tooltips from "./Tooltips";
import { startOfToday } from "./perf";
import type { DiskTrend, GrowthInfo, Live, PerfHistory } from "./types";
import { abbrev, fmt, fmtAt, fmtDelta, fmtRate, pct } from "./util";

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
}: {
  on: boolean;
  label: string;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      className={`switch${on ? " on" : ""}`}
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
  const [disk, setDisk] = useState<DiskTrend | null>(null);
  const [growth, setGrowth] = useState<GrowthInfo | null>(null);
  const [home, setHome] = useState("");
  const [login, setLogin] = useState<[boolean, boolean]>([false, false]);
  const [open, setOpen] = useState(true);

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
      .growth("/", 7)
      .then(setGrowth)
      .catch(() => {});
    api
      .state()
      .then((s) => setHome(s.home))
      .catch(() => {});
    api
      .loginItem()
      .then(setLogin)
      .catch(() => {});
  }, []);
  useEffect(() => {
    load();
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
  const rescan = {
    label: "Rescan disk",
    keys: "⌘R",
    run: () => api.startScan(),
    tip: "Scan the disk again now",
  };
  const primary = !attention
    ? rescan
    : about === "disk"
      ? {
          label: "Show the disk map",
          keys: "⌘3",
          run: () => api.showMain("disk"),
          tip: "Open the map to see what is filling the disk",
        }
      : {
          label: `Show what's using ${about === "cpu" ? "the CPU" : "memory"}`,
          keys: "⌘2",
          run: () => api.showMain("performance"),
          tip: "Open Performance to see which apps are responsible",
        };
  const paused = live?.paused_until != null;
  const third = attention
    ? rescan
    : paused
      ? {
          label: "Resume recording",
          keys: "",
          run: () => api.pauseRecording(0),
          tip: "Start recording again",
        }
      : {
          label: "Pause recording for an hour",
          keys: "",
          run: () => api.pauseRecording(60),
          tip: "Stop recording for an hour; it resumes by itself",
        };

  // The shortcuts the menu shows work while it is open.
  useEffect(() => {
    const keys: Record<string, () => unknown> = {
      r: () => api.startScan(),
      k: () => api.openAsk(),
      o: () => api.showMain(),
      ",": () => api.showMain("settings"),
      q: () => api.quit(),
      "2": () => api.showMain("performance"),
      "3": () => api.showMain("disk"),
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
  const series = (k: "cpu" | "mem"): Pt[] => live.samples.map((s) => ({ t: s.t / 1000, v: s[k] }));
  const diskPts: Pt[] = (disk?.points ?? []).map((p) => ({ t: p.ts, v: p.used }));
  const diskNow = diskPts[diskPts.length - 1]?.v ?? 0;
  const diskTotal = disk?.points[disk.points.length - 1]?.total ?? 0;
  const startOfDay = (disk?.points ?? []).find((p) => p.ts >= today);
  const diskToday = startOfDay ? diskNow - startOfDay.used : 0;
  const memWarn = about === "memory" || about === "swap";
  const memNewMark = memWarn && tm.mem && latest && latest.mem >= tm.mem.value * 0.98;

  const rows = [
    {
      name: "CPU",
      note: cpuHigh ? `Today's high ${pct(cpuHigh.v)} · ${fmtAt(cpuHigh.t)}` : "Recording",
      warn: about === "cpu",
      value: latest ? pct(latest.cpu) : "–",
      spark: (
        <Spark
          pts={series("cpu")}
          max={100}
          mark={tm.cpu?.value}
          tone={about === "cpu" ? "warn" : "tide"}
          label="CPU, last 15 minutes"
        />
      ),
    },
    {
      name: "Memory",
      note: memNewMark
        ? `New tidemark · swap ${fmt(latest.swap)}`
        : memHigh
          ? `Today's high ${fmt(memHigh.v)}`
          : "Recording",
      warn: memWarn,
      value: latest ? fmt(latest.mem) : "–",
      spark: (
        <Spark
          pts={series("mem")}
          max={live.mem_total}
          mark={tm.mem?.value}
          tone={memWarn ? "warn" : "tide"}
          label="Memory used, last 15 minutes"
        />
      ),
    },
    {
      name: "Network",
      note: latest ? `↑ ${fmtRate(latest.net_out)} up` : "Recording",
      warn: false,
      value: latest ? `↓ ${fmtRate(latest.net_in)}` : "–",
      spark: (
        <Spark
          pts={live.samples.map((x) => ({ t: x.t / 1000, v: x.net_in }))}
          max={Math.max(1, ...live.samples.map((x) => x.net_in))}
          label="Download, last 15 minutes"
        />
      ),
    },
    {
      name: "Disk",
      note: diskToday > 1e8 ? `${fmtDelta(diskToday)} today` : "Steady today",
      warn: about === "disk",
      value: diskTotal ? `${fmt(diskTotal - diskNow)} free` : "–",
      spark: (
        <Spark
          pts={diskPts}
          min={diskPts.length ? Math.min(...diskPts.map((p) => p.v)) * 0.995 : 0}
          max={diskPts.length ? Math.max(...diskPts.map((p) => p.v)) : 1}
          mark={disk?.high?.used}
          tone={about === "disk" ? "warn" : "tide"}
          label="Disk used, last 7 days"
        />
      ),
    },
  ];

  const top = (about === "cpu" ? live.apps_cpu : live.apps_mem).slice(0, 3);
  const landed = (growth?.landed ?? []).filter((g) => g.delta > 0);
  const grew = Math.max(0, growth?.total ?? 0);
  const memHog = live.apps_mem[0];
  const week =
    memWarn && memHog && live.mem_total
      ? {
          warn: true,
          text: `${memHog.app} holds ${Math.round((100 * memHog.mem) / live.mem_total)}% of your memory`,
          note: "Quitting it when you're not using it frees the most. Ask for details.",
          go: () => api.showMain("performance"),
        }
      : grew > 1e8
        ? {
            warn: false,
            text: `Disk grew ${fmt(grew)} this week`,
            note: landed[0]
              ? `Mostly ${abbrev(landed[0].path, home)} (${fmtDelta(landed[0].delta)}). Open the map to see it.`
              : "Open the map to see where.",
            go: () => api.showMain("disk"),
          }
        : {
            warn: false,
            text: growth?.from ? "The disk held steady this week" : "Disk growth shows after a day",
            note: growth?.from
              ? "Nothing grew by more than 100 MB."
              : "Tidemark compares one scan with the next.",
            go: () => api.showMain("trends"),
          };

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
            title={
              r.name === "Disk"
                ? "Open the disk map"
                : `Open Performance for ${r.name.toLowerCase()} in detail`
            }
            onClick={() => {
              void api.showMain(r.name === "Disk" ? "disk" : "performance");
              hide();
            }}
          >
            <span className="tname">
              <span className="nm">{r.name}</span>
              <span className={`tiny${r.warn ? " warnt" : " muted"}`}>{r.note}</span>
            </span>
            {r.spark}
            <span className={`tval${r.warn ? " warnt" : ""}`}>{r.value}</span>
          </button>
        ))}
      </div>

      <div className="tsec tops">
        <div className="muted tiny tl">Using the most right now</div>
        {top.map((a) => (
          <button
            key={a.app}
            className="menu-item"
            title="Open Performance to see every app"
            onClick={() => {
              void api.showMain("performance");
              hide();
            }}
          >
            <span className="grow ell">{a.app}</span>
            <span className="tiny muted">
              {fmt(a.mem)} · {pct(a.cpu)} CPU
            </span>
          </button>
        ))}
      </div>

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

      <div className="tactions">
        <Item
          label={primary.label}
          keys={primary.keys}
          strong
          onClick={primary.run}
          tip={primary.tip}
        />
        <Item
          label="Ask Tidemark…"
          keys="⌘K"
          onClick={() => api.openAsk()}
          tip="Ask an AI about your disk or performance"
        />
        <Item label={third.label} keys={third.keys} onClick={third.run} tip={third.tip} />
        <div className="msep" />
        {login[0] && (
          <>
            <div className="menu-item static">
              <span className="grow">Open at Login</span>
              <Switch
                label="Open at Login"
                on={login[1]}
                onChange={(v) => void api.setLoginItem(v).then((r) => setLogin([true, r]))}
              />
            </div>
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
          tip="Quit. Nothing is recorded until Tidemark is opened again"
        />
      </div>
      <div className="tfoot">Tidemark keeps recording when the window is closed.</div>
    </div>
  );
}
