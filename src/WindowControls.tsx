// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The main window's own minimise, maximise and close buttons on Windows, where it has no native
// title bar (src-tauri/src/lib.rs turns it off): at the top right, where Windows puts its own,
// as wide as those and as tall as the top bar above its divider, over every screen and the Ask
// and Help drawers (which open under the top bar there). Close goes through the window's close
// request, the same path as the native X, so lib.rs hides the window to the notification area
// as before. Nothing here on macOS, whose window keeps its traffic lights.
//
// Windows 11 offers its snap layouts over the maximise button only to a window that answers the
// system's hit-test there, which the page can't: src-tauri/src/snap_win.rs puts a window of its
// own over the button, told where it is here, does the click, and says when the pointer is on it
// (the page gets no hover of its own there).

import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { isMac } from "./keys";

const Glyph = ({ d }: { d: string }) => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" shapeRendering="crispEdges">
    <path d={d} fill="none" stroke="currentColor" strokeWidth="1" />
  </svg>
);

export default function WindowControls() {
  const [maximised, setMaximised] = useState(false);
  const [hover, setHover] = useState(false);
  const max = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (isMac) return;
    const w = getCurrentWindow();
    // The snap-layout window goes over the button, in the window's physical pixels.
    const place = () => {
      const b = max.current?.getBoundingClientRect();
      const r = window.devicePixelRatio || 1;
      void api
        .windowMaxButton(
          b
            ? [
                Math.round(b.left * r),
                Math.round(b.top * r),
                Math.round(b.width * r),
                Math.round(b.height * r),
              ]
            : null,
        )
        .catch(() => {});
    };
    const read = () => {
      void w.isMaximized().then(setMaximised, () => {});
      place();
    };
    read();
    window.addEventListener("resize", read);
    // The button's own size can change without the window's (the page's zoom).
    const sized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    if (max.current) sized?.observe(max.current);
    const offHover = api.onMaxButtonHover(setHover);
    const offs = [w.onResized(read), w.onScaleChanged(read)].map((o) => o.catch(() => () => {}));
    return () => {
      sized?.disconnect();
      window.removeEventListener("resize", read);
      offHover();
      offs.forEach((o) => void o.then((f) => f()));
      void api.windowMaxButton(null).catch(() => {});
    };
  }, []);
  if (isMac) return null;
  const w = () => getCurrentWindow();
  return (
    <div className="winctl" role="group" aria-label="Window">
      <button
        type="button"
        tabIndex={-1}
        title="Minimise"
        aria-label="Minimise"
        onClick={() => void w().minimize()}
      >
        <Glyph d="M0 5.5h10" />
      </button>
      <button
        ref={max}
        type="button"
        tabIndex={-1}
        className={hover ? "hover" : undefined}
        title={maximised ? "Restore" : "Maximise"}
        aria-label={maximised ? "Restore" : "Maximise"}
        onClick={() => void w().toggleMaximize()}
      >
        {maximised ? (
          <Glyph d="M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" />
        ) : (
          <Glyph d="M0.5 0.5h9v9h-9z" />
        )}
      </button>
      <button
        type="button"
        tabIndex={-1}
        className="close"
        title="Close"
        aria-label="Close"
        onClick={() => void w().close()}
      >
        <Glyph d="M0 0l10 10M10 0L0 10" />
      </button>
    </div>
  );
}
