// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The update dialog (src-tauri/src/updates.rs): a new version on offer with Install and Restart,
// Later or Skip This Version, then the download's progress and the restart; and what Check for
// Updates… has to say otherwise, with OK. A modal <dialog>, so the window behind it is inert and
// Esc is Later (or OK).

import { useEffect, useRef, useState } from "react";
import { api, on } from "./api";
import { Mark } from "./Mark";
import { mdToHtml } from "./md";
import type { UpdateMessage, UpdateOffer, UpdateProgress } from "./types";
import { fmt } from "./util";

interface Shown {
  offer: UpdateOffer | null;
  /// Set once Install and Restart is chosen.
  progress: UpdateProgress | null;
  /// Waiting their turn behind the offer, or each other.
  messages: UpdateMessage[];
}

const NONE: Shown = { offer: null, progress: null, messages: [] };

/// 2026-10-11 as "11 Oct 2026", on the day it says rather than the UTC one.
function day(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  const t = new Date(y, m - 1, d);
  if (Number.isNaN(t.getTime())) return date;
  return t.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

export default function UpdateDialog() {
  const [shown, setShown] = useState<Shown>(NONE);
  const ref = useRef<HTMLDialogElement>(null);
  const main = useRef<HTMLButtonElement>(null);
  const { offer, progress } = shown;
  const message = offer ? null : (shown.messages[0] ?? null);
  const open = !!(offer || message);

  useEffect(() => {
    // A screenshot's scene shows the dialog as it gives it, and nothing from the app.
    let scene = false;
    void api
      .scene()
      .then((sc) => {
        if (!sc) return api.updateOffered().then((o) => o && setShown((s) => ({ ...s, offer: o })));
        scene = true;
        const u = sc.update;
        if (u)
          setShown({
            offer: u.offer ?? null,
            progress: u.progress ?? null,
            messages: u.message ? [u.message] : [],
          });
      })
      .catch(() => {});
    const offs = [
      on<UpdateOffer>("update-offer", (o) => {
        if (!scene) setShown((s) => ({ ...s, offer: o, progress: null }));
      }),
      on<UpdateProgress>("update-progress", (p) => {
        if (!scene) setShown((s) => (s.offer ? { ...s, progress: p } : s));
      }),
      // A message during the install is how it ended (it waits, or failed): the offer goes.
      on<UpdateMessage>("update-message", (m) => {
        if (scene) return;
        setShown((s) => ({
          offer: s.progress ? null : s.offer,
          progress: s.progress ? null : s.progress,
          messages: [...s.messages, m],
        }));
      }),
    ];
    return () => offs.forEach((off) => off());
  }, []);

  // Opened and closed with the state; the main button takes the focus whenever what is shown
  // changes, as a Mac alert's default button does.
  const view = offer ? `offer ${offer.version}` : message ? `message ${message.title}` : "";
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
    if (open) main.current?.focus();
  }, [open, view]);

  const busy = !!progress;
  const later = () => {
    void api.updateLater();
    setShown((s) => ({ ...s, offer: null, progress: null }));
  };
  const skip = () => {
    void api.updateSkip();
    setShown((s) => ({ ...s, offer: null, progress: null }));
  };
  const install = () => {
    setShown((s) => ({ ...s, progress: { phase: "downloading", done: 0, total: null } }));
    ref.current?.focus();
    void api.updateInstall();
  };
  const ok = () => setShown((s) => ({ ...s, messages: s.messages.slice(1) }));

  let body = null;
  if (offer) {
    const notes = offer.notes?.trim();
    body = (
      <>
        <div className="upd-head">
          <Mark size={48} />
          <div>
            <h2 id="upd-title" className="ttl">
              {offer.name} {offer.version} is available
            </h2>
            <p id="upd-text" className="upd-sub">
              You have {offer.current}.{offer.date && ` Released ${day(offer.date)}.`}
            </p>
          </div>
        </div>
        {notes && (
          // Release notes in Ask's answer style: the same Markdown, the same look.
          <div
            className="upd-notes ask-a"
            aria-label="Release notes"
            dangerouslySetInnerHTML={{ __html: mdToHtml(notes) }}
          />
        )}
        {progress ? (
          <Installing p={progress} />
        ) : (
          <div className="upd-buttons">
            <button type="button" className="btn" onClick={skip}>
              Skip This Version
            </button>
            <div className="grow" />
            <button type="button" className="btn" onClick={later}>
              Later
            </button>
            <button ref={main} type="button" className="btn primary" onClick={install}>
              Install and Restart
            </button>
          </div>
        )}
      </>
    );
  } else if (message) {
    body = (
      <>
        <div className="upd-head">
          <Mark size={48} />
          <div>
            <h2 id="upd-title" className="ttl">
              {message.title}
            </h2>
            <p id="upd-text" className="upd-sub">
              {message.text}
            </p>
          </div>
        </div>
        <div className="upd-buttons">
          <div className="grow" />
          <button ref={main} type="button" className="btn primary" onClick={ok}>
            OK
          </button>
        </div>
      </>
    );
  }

  return (
    <dialog
      ref={ref}
      className="upd"
      tabIndex={-1}
      aria-labelledby="upd-title"
      aria-describedby="upd-text"
      aria-busy={busy}
      // Esc: Later, or OK; nothing once the install has started.
      onCancel={(e) => {
        e.preventDefault();
        if (busy) return;
        if (offer) later();
        else ok();
      }}
      // The window's own shortcuts (Esc closing Ask, ⌘1-4) stay out of it while it is up.
      onKeyDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.stopPropagation()}
    >
      {body}
    </dialog>
  );
}

function Installing({ p }: { p: UpdateProgress }) {
  const known = p.phase === "downloading" && p.total;
  const share = known ? Math.min(1, p.done / (p.total as number)) : 0;
  let text;
  if (p.phase === "restarting") text = "Restarting…";
  else if (p.phase === "installing") text = "Installing…";
  else if (known) text = `Downloading… ${fmt(p.done)} of ${fmt(p.total as number)}`;
  else text = p.done ? `Downloading… ${fmt(p.done)}` : "Downloading…";
  return (
    <div className="upd-progress" role="status">
      <div
        className={`bar${known ? "" : " indet"}`}
        role="progressbar"
        aria-label="Update"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={known ? Math.round(share * 100) : undefined}
      >
        <i style={known ? { width: `${share * 100}%` } : undefined} />
      </div>
      <span className="nums">{text}</span>
    </div>
  );
}
