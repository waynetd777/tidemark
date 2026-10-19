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
import { isMac } from "./keys";

// The web view's own right-click menu (WebKit's, or WebView2's) offers Reload, which throws away the app's state. Keep it only
// where it's useful — in text fields and on selected text — and leave custom menus to their owners.
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (t?.closest("input, textarea, [contenteditable]:not([contenteditable='false'])")) return;
  if (window.getSelection()?.toString()) return;
  e.preventDefault();
});

followTheme();

// macOS's window has no title bar: the page leaves room for its traffic lights (styles.css .mac).
document.documentElement.classList.toggle("mac", isMac);

// Endless animations cost a wakeup and a repaint each frame: the Now dots' breathing ring stops
// while the window is behind others, and everything stops while it's hidden.
const idle = () => {
  const root = document.documentElement;
  root.toggleAttribute("data-hidden", document.hidden);
  root.toggleAttribute("data-idle", document.hidden || !document.hasFocus());
};
window.addEventListener("focus", idle);
window.addEventListener("blur", idle);
document.addEventListener("visibilitychange", idle);
idle();

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
