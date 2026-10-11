// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// How a measure looks when the status is about it: amber for a warning, red when critical,
// with the status's label beside it so colour is never the only sign. Every other measure
// keeps the tide colour. One rule for the tray, Now and Performance.

import type { Status } from "./types";

export type Tone = "tide" | "warn" | "bad";

/// The measures each status is about, by the names the cards use.
const CARDS: Record<string, string[]> = {
  cpu: ["CPU"],
  memory: ["Memory"],
  swap: ["Memory"],
  disk: ["Disk space"],
  energy: ["Energy"],
};

export interface Flag {
  tone: Tone;
  /// The text colour class for the measure's number.
  cls: "warnt" | "badt";
  /// "Getting tight", "Critical", "Busy"...
  label: string;
}

/// The part of the status about a card: the headline's, or one of the others needing attention.
export function statusFor(status: Status | null | undefined, card: string): Status | null {
  if (!status || status.level !== "attention") return null;
  return (
    [status, ...(status.also ?? [])].find((s) => (CARDS[s.about] ?? []).includes(card)) ?? null
  );
}

/// The flag for a card, or null when the status isn't about it.
export function flagFor(status: Status | null | undefined, card: string): Flag | null {
  const s = statusFor(status, card);
  if (!s) return null;
  const bad = s.severity === "critical";
  return { tone: bad ? "bad" : "warn", cls: bad ? "badt" : "warnt", label: s.label };
}

/// The colour for a tone, as charts draw it.
export function toneColor(tone: Tone | undefined): string {
  return tone === "warn" ? "var(--warn)" : tone === "bad" ? "var(--bad)" : "var(--tide)";
}
