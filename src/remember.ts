// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Choices remembered between launches: a range picked, the screen last open. Per Mac, in
// localStorage; a choice that can't be read back (a private window, an old value) is the default.

import { useCallback, useState } from "react";

/// Screenshots (tools/screenshots.py) show the defaults, not what this Mac's dev build chose.
export const sceneMode = { on: false };

function load<T>(key: string, fallback: T, ok: (v: unknown) => v is T): T {
  if (sceneMode.on) return fallback;
  try {
    const raw = localStorage.getItem(key);
    if (raw == null) return fallback;
    const v: unknown = JSON.parse(raw);
    return ok(v) ? v : fallback;
  } catch {
    return fallback;
  }
}

/// Like useState, but the value is kept for the next launch. `ok` says whether a stored value
/// is still one the screen can use.
export function useRemembered<T>(
  key: string,
  fallback: T,
  ok: (v: unknown) => v is T,
): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => load(key, fallback, ok));
  const set = useCallback(
    (nv: T) => {
      setV(nv);
      if (sceneMode.on) return;
      try {
        localStorage.setItem(key, JSON.stringify(nv));
      } catch {
        // Not remembered; it still applies until the app quits.
      }
    },
    [key],
  );
  return [v, set];
}

/// An index into a list of `n` choices.
export const index =
  (n: number) =>
  (v: unknown): v is number =>
    typeof v === "number" && Number.isInteger(v) && v >= 0 && v < n;
