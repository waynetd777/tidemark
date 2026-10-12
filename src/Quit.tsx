// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The quit question (tray::confirm_quit): ⌘Q, the app menu's Quit or the menu-bar window's ask
// before quitting. The update dialog's panel (Update.tsx), with Quit as the default button and
// Esc as Cancel.

import { useEffect, useRef, useState } from "react";
import { api, on } from "./api";
import { Mark } from "./Mark";

export default function QuitDialog() {
  // Whether the background helper keeps recording; null while nothing is asked.
  const [helperOn, setHelperOn] = useState<boolean | null>(null);
  const ref = useRef<HTMLDialogElement>(null);
  const main = useRef<HTMLButtonElement>(null);
  const open = helperOn !== null;

  useEffect(() => on<boolean>("confirm-quit", setHelperOn), []);

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
    if (open) main.current?.focus();
  }, [open]);

  const cancel = () => {
    setHelperOn(null);
    void api.quitCancelled();
  };

  return (
    <dialog
      ref={ref}
      className="upd"
      tabIndex={-1}
      aria-labelledby="quit-title"
      aria-describedby="quit-text"
      onCancel={(e) => {
        e.preventDefault();
        cancel();
      }}
      // The window's own shortcuts stay out of it while it is up.
      onKeyDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.stopPropagation()}
    >
      {open && (
        <>
          <div className="upd-head">
            <Mark size={48} />
            <div>
              <h2 id="quit-title" className="ttl">
                Quit Tidemark?
              </h2>
              <p id="quit-text" className="upd-sub">
                {helperOn
                  ? "The background helper keeps recording."
                  : "Nothing is recorded until Tidemark is opened again."}
              </p>
            </div>
          </div>
          <div className="upd-buttons">
            <div className="grow" />
            <button type="button" className="btn" onClick={cancel}>
              Cancel
            </button>
            <button
              ref={main}
              type="button"
              className="btn primary"
              onClick={() => void api.quitNow()}
            >
              Quit
            </button>
          </div>
        </>
      )}
    </dialog>
  );
}
