// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Tray from "./Tray";
import "./styles.css";
import { api } from "./api";
import { sceneMode } from "./remember";
import { applyTheme, followTheme } from "./theme";

followTheme();

/// How long the splash stays up at minimum, so a fast start reads as a deliberate opening
/// rather than a blink.
const MIN_SPLASH_MS = 900;
const splashShownAt = Date.now();

/// Fade out the splash that index.html painted, once there is a real screen to replace it.
export function hideSplash() {
  const el = document.getElementById("splash");
  if (!el || el.classList.contains("gone") || el.dataset.going) return;
  const wait = Math.max(0, MIN_SPLASH_MS - (Date.now() - splashShownAt));
  el.dataset.going = "1";
  setTimeout(() => {
    el.classList.add("gone");
    setTimeout(() => el.remove(), 250);
  }, wait);
}

/// The menu-bar window loads the same page with ?view=tray (tauri.conf.json).
const tray = new URLSearchParams(location.search).get("view") === "tray";

// A screenshot's scene (tools/screenshots.py) sets its own theme and shows the defaults, so it
// is known before the first render.
void api
  .scene()
  .catch(() => null)
  .then((sc) => {
    if (sc) {
      sceneMode.on = true;
      if (sc.theme) applyTheme(sc.theme);
    }
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
      <React.StrictMode>{tray ? <Tray /> : <App />}</React.StrictMode>,
    );
  });
