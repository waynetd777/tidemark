// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// Settings: three things to choose. Everything else looks after itself.

import { useEffect, useState, type ReactNode } from "react";
import { api, type Helper } from "./api";
import { usePoll } from "./perf";
import { saveTheme } from "./theme";
import { Switch } from "./Tray";
import type { AppInfo, Settings as S } from "./types";
import { fmt } from "./util";

export default function Settings({
  info,
  settings,
  onChange,
  bar,
}: {
  info: AppInfo | null;
  settings: S | null;
  onChange: (s: S) => void;
  bar: (left: ReactNode) => ReactNode;
}) {
  const [login, setLogin] = useState<[boolean, boolean]>([false, false]);
  useEffect(() => {
    api
      .loginItem()
      .then(setLogin)
      .catch(() => {});
  }, []);
  const hist = usePoll(api.historyInfo, 30_000);
  const [helper, setHelper] = useState<Helper | null>(null);
  useEffect(() => {
    api
      .helper()
      .then(setHelper)
      .catch(() => {});
  }, []);
  const save = (s: S) =>
    api
      .saveSettings(s)
      .then(onChange)
      .catch(() => {});

  return (
    <>
      {bar(<span className="ttl">Settings</span>)}
      <div className="page narrow">
        <p className="muted">A few things to choose. Everything else looks after itself.</p>
        {settings && (
          <div className="group">
            <div className="set">
              <span className="t">
                <span>Keep history for</span>
                <span className="muted">
                  Every app, every minute. Disk totals and top folders are kept for a year
                  regardless.
                </span>
              </span>
              <select
                aria-label="Keep history for"
                title="How many days of minute-by-minute history to keep"
                value={settings.history_days}
                onChange={(e) => void save({ ...settings, history_days: Number(e.target.value) })}
              >
                <option value={14}>14 days</option>
                <option value={7}>7 days</option>
                <option value={3}>3 days</option>
              </select>
            </div>
            <div className="set">
              <span className="t">
                <span>Scan the disk</span>
                <span className="muted">
                  {settings.scan === "daily"
                    ? "Once a day, when the Mac is idle and on power."
                    : settings.scan === "hourly"
                      ? "Every hour. A full scan takes a minute of hard work, so this costs some battery."
                      : "Only when you press Reload. Growth is shown between the scans you run."}
                </span>
              </span>
              <div className="seg" role="group" aria-label="Scan the disk">
                {(["daily", "hourly", "off"] as const).map((v) => (
                  <button
                    key={v}
                    className={settings.scan === v ? "on" : ""}
                    aria-pressed={settings.scan === v}
                    onClick={() => void save({ ...settings, scan: v })}
                    title={
                      v === "daily"
                        ? "One full scan a day, when idle and on power"
                        : v === "hourly"
                          ? "A full scan every hour"
                          : "Scan only when you press Reload"
                    }
                  >
                    {v[0].toUpperCase() + v.slice(1)}
                  </button>
                ))}
              </div>
            </div>
            <div className="set">
              <span className="t">
                <span>Appearance</span>
                <span className="muted">Match the Mac, or always light or dark.</span>
              </span>
              <div className="seg" role="group" aria-label="Appearance">
                {(
                  [
                    ["system", "Match the Mac"],
                    ["light", "Light"],
                    ["dark", "Dark"],
                  ] as const
                ).map(([v, label]) => (
                  <button
                    key={v}
                    className={settings.theme === v ? "on" : ""}
                    aria-pressed={settings.theme === v}
                    onClick={() => {
                      saveTheme(v);
                      void save({ ...settings, theme: v });
                    }}
                    title={
                      v === "system"
                        ? "Follow the Mac's light or dark appearance"
                        : `Always ${label.toLowerCase()}`
                    }
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
            <div className="set">
              <span className="t">
                <span>Keep recording when quit</span>
                <span className="muted">
                  {!helper?.available
                    ? "Available in the installed app (make install-app), not in a development build."
                    : helper.needs_approval
                      ? "macOS needs you to allow it: System Settings › General › Login Items › Allow in the Background."
                      : "A small background helper records while Tidemark is quit, and starts when you log in."}
                </span>
              </span>
              <span className={helper?.available ? "" : "disabled"}>
                <Switch
                  label="Keep recording when quit"
                  on={!!helper?.on}
                  onChange={(v) => {
                    if (helper?.available) void api.setHelper(v).then(setHelper);
                  }}
                />
              </span>
            </div>
            <div className="set">
              <span className="t">
                <span>Open at Login</span>
                <span className="muted">
                  {login[0]
                    ? "Tidemark records only while it is running."
                    : "Available in the installed app (make install-app), not in a development build."}
                </span>
              </span>
              <span className={login[0] ? "" : "disabled"}>
                <Switch
                  label="Open at Login"
                  on={login[1]}
                  onChange={(v) => {
                    if (login[0]) void api.setLoginItem(v).then((r) => setLogin([true, r]));
                  }}
                />
              </span>
            </div>
          </div>
        )}
        <div className="setfoot muted small">
          <span>
            History uses {hist ? fmt(hist.bytes) : "…"} in ~/Library/Application Support/Tidemark
          </span>
          {info && (
            <span className="row">
              <span className={`sdot${info.full_disk_access ? "" : " warn"}`} />
              {info.full_disk_access ? (
                "Full Disk Access granted"
              ) : (
                <button
                  className="link"
                  onClick={() => void api.openPrivacySettings()}
                  title="Open System Settings › Privacy & Security › Full Disk Access"
                >
                  Give Full Disk Access…
                </button>
              )}
            </span>
          )}
        </div>
      </div>
    </>
  );
}
