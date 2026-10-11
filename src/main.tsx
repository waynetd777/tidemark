// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Tray from "./Tray";
import "./styles.css";
import { api } from "./api";
import { sceneMode } from "./remember";
import { applyTheme, followTheme } from "./theme";
import "./splash";

followTheme();

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
