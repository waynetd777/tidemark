// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Settings: three things to choose. Everything else looks after itself.

import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState, type ReactNode } from "react";
import { api, type Helper } from "./api";
import { Globe } from "./icons";
import { Mark } from "./Mark";
import { usePoll } from "./perf";
import { saveTheme } from "./theme";
import { Switch } from "./Tray";
import type { AppInfo, Settings as S } from "./types";
import { fmt } from "./util";
import { DATA_DIR, isMac, LOG_IN, MACHINE, MENU_BAR, OS } from "./keys";

/// Why a switch that needs the installed app is off in a development build.
const NOT_INSTALLED = isMac
  ? "Available in the installed app (make install-app), not in a development build."
  : "Available in the installed app, not in a development build.";

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
  // Why System Settings couldn't be opened, under the link.
  const [fdaErr, setFdaErr] = useState<string | null>(null);
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion()
      .then(setVersion)
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
                <span>Keep recording when quit</span>
                <span className="muted">
                  {!helper?.available
                    ? NOT_INSTALLED
                    : helper.needs_approval
                      ? isMac
                        ? "Switched off in System Settings › General › Login Items › Allow in the Background. Switch Tidemark on there to keep recording."
                        : "Switched off in Windows Settings › Apps › Startup. Switch Tidemark Helper on there to keep recording."
                      : `A small background helper records while Tidemark is quit, and starts when you ${LOG_IN}.`}
                </span>
              </span>
              <span className={helper?.available ? "" : "disabled"}>
                <Switch
                  label="Keep recording when quit"
                  disabled={!helper?.available}
                  on={!!helper?.on}
                  onChange={(v) => {
                    if (helper?.available)
                      void api
                        .setHelper(v)
                        .then(setHelper)
                        .catch(() => {});
                  }}
                />
              </span>
            </div>
            <div className="set">
              <span className="t">
                <span>Open at Login</span>
                <span className="muted">
                  {login[0]
                    ? helper?.on
                      ? `Opens Tidemark in the ${MENU_BAR} when you ${LOG_IN}. The background helper records either way.`
                      : `Tidemark records only while it is running, so this keeps it recording from the moment you ${LOG_IN}.`
                    : NOT_INSTALLED}
                </span>
              </span>
              <span className={login[0] ? "" : "disabled"}>
                <Switch
                  label="Open at Login"
                  disabled={!login[0]}
                  on={login[1]}
                  onChange={(v) => {
                    if (login[0])
                      void api
                        .setLoginItem(v)
                        .then((r) => setLogin([true, r]))
                        .catch(() => {});
                  }}
                />
              </span>
            </div>
            <div className="set">
              <span className="t">
                <span>Keep every minute for</span>
                <span className="muted">
                  Every measure and app, minute by minute, for Recent. A row a day of each, and the
                  disk's totals and top folders, are kept for a year regardless, for Trends.
                </span>
              </span>
              <select
                aria-label="Keep every minute for"
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
                    ? `Once a day, when the ${MACHINE} is idle and on power (on battery too once the last scan is two days old), and at launch if the last scan is over an hour old.`
                    : settings.scan === "hourly"
                      ? "Every hour. A full scan takes about a minute of hard work, so this costs some battery."
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
                        ? "One full scan a day when idle and on power (on battery after two days), and at launch if the last is over an hour old"
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
                <span className="muted">Match {OS}, or always light or dark.</span>
              </span>
              <div className="seg" role="group" aria-label="Appearance">
                {(
                  [
                    ["system", `Match ${OS}`],
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
                        ? `Follow ${OS}'s light or dark appearance`
                        : `Always ${label.toLowerCase()}`
                    }
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          </div>
        )}
        <div className="group about">
          <Mark size={48} />
          <div className="about-t">
            <span className="about-name">
              Tidemark{version && <span className="muted"> {version}</span>}
            </span>
            <span className="muted">The high-water marks of your {MACHINE}.</span>
          </div>
          <button
            className="btn"
            title="Open Tidemark's website in the browser"
            onClick={() => void api.openWebsite()}
          >
            <Globe />
            Website
          </button>
          {/* Windows has no app menu to carry it. */}
          {!isMac && (
            <button
              className="btn"
              title="See whether a newer Tidemark is out"
              onClick={() => void api.checkForUpdates()}
            >
              Check for Updates…
            </button>
          )}
        </div>
        <div className="setfoot muted small">
          <span>
            History uses {hist ? fmt(hist.bytes) : "…"} in {DATA_DIR}
          </span>
          {/* Windows has no Full Disk Access to give. */}
          {info && isMac && (
            <span className="row">
              <span className={`sdot${info.full_disk_access ? "" : " warn"}`} />
              {info.full_disk_access ? (
                "Full Disk Access granted"
              ) : (
                <button
                  className="link"
                  onClick={() => {
                    setFdaErr(null);
                    api
                      .openPrivacySettings()
                      .catch((e: unknown) =>
                        setFdaErr(`Couldn't open System Settings: ${String(e)}`),
                      );
                  }}
                  title="Open System Settings › Privacy & Security › Full Disk Access"
                >
                  Give Full Disk Access…
                </button>
              )}
              {fdaErr && (
                <span role="alert" style={{ color: "var(--bad)", marginLeft: 6 }}>
                  {fdaErr}
                </span>
              )}
            </span>
          )}
        </div>
      </div>
    </>
  );
}
