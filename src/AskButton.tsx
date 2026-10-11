// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// A small Ask button beside something worth asking about: an incident, a crash, a flagged chart.
// It opens Ask with a question and the thing's details; disabled, with the reason, when no AI
// tool is installed.

import { useEffect, useState } from "react";
import { useAssistant } from "./assistant";

export function AskIcon() {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M2.5 3.5A1.5 1.5 0 0 1 4 2h8a1.5 1.5 0 0 1 1.5 1.5v6A1.5 1.5 0 0 1 12 11H7l-3.2 2.7V11H4a1.5 1.5 0 0 1-1.5-1.5z" />
    </svg>
  );
}

export function AskButton({ onAsk, title }: { onAsk: () => void | Promise<void>; title: string }) {
  const assistant = useAssistant();
  // A crash's reports are read first, which takes a moment.
  const [busy, setBusy] = useState(false);
  // Why Ask couldn't open (a crash's reports unreadable, say), for a few seconds.
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    if (!failed) return;
    const id = window.setTimeout(() => setFailed(null), 6000);
    return () => window.clearTimeout(id);
  }, [failed]);
  return (
    <span
      className="tipwrap"
      title={assistant.available ? undefined : (assistant.reason ?? undefined)}
    >
      <button
        className="btn small askbtn"
        disabled={!assistant.available || busy}
        title={title}
        onClick={() => {
          setBusy(true);
          setFailed(null);
          // Through then(), so a throw as well as a rejection clears busy.
          void Promise.resolve()
            .then(onAsk)
            .catch((e: unknown) => setFailed(`Couldn't open Ask: ${String(e)}`))
            .finally(() => setBusy(false));
        }}
      >
        <AskIcon />
        Ask
      </button>
      {failed && (
        <span className="small" role="alert" style={{ color: "var(--bad)", marginLeft: 6 }}>
          {failed}
        </span>
      )}
    </span>
  );
}
