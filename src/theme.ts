// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Appearance: match the Mac, or always light or dark. Kept in localStorage, which the main window
// and the menu-bar window share, and applied as data-theme on <html> (styles.css keys its dark
// colours on it). index.html applies it before the first paint so nothing flashes.

import { getCurrentWindow } from "@tauri-apps/api/window";

export type Theme = "system" | "light" | "dark";
const KEY = "theme";

export function savedTheme(): Theme {
  try {
    const t = localStorage.getItem(KEY);
    return t === "light" || t === "dark" ? t : "system";
  } catch {
    return "system";
  }
}

/// Applies a theme to this window: its colours, and the native window's appearance (title bar,
/// menus, scroll bars), which would otherwise follow the Mac.
export function applyTheme(t: Theme) {
  if (t === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  void getCurrentWindow()
    .setTheme(t === "system" ? null : t)
    .catch(() => {});
}

export function saveTheme(t: Theme) {
  try {
    localStorage.setItem(KEY, t);
  } catch {
    // Not remembered; it still applies until the app quits.
  }
  applyTheme(t);
}

/// Applies the saved theme now, and again whenever the other window changes it.
export function followTheme() {
  applyTheme(savedTheme());
  window.addEventListener("storage", (e) => {
    if (e.key === KEY) applyTheme(savedTheme());
  });
}
