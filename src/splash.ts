// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The splash index.html paints, and its fade once there is a screen to show. Apart from
// main.tsx so App can import it: importing main.tsx made every hot reload of App run main.tsx
// again, putting a second React root on each window.

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
