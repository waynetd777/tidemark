// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

// Small line icons for what Tidemark measures, beside each measure's label. Drawn on a 16-unit
// grid in the current text colour, like the sidebar's icons.

import type { Metric } from "./metrics";

const PATHS: Record<Metric, React.ReactNode> = {
  // A chip with pins.
  cpu: (
    <>
      <rect x="4" y="4" width="8" height="8" rx="1.5" />
      <rect x="6.5" y="6.5" width="3" height="3" rx="0.5" />
      <path d="M6 1.8V4M10 1.8V4M6 12v2.2M10 12v2.2M1.8 6H4M1.8 10H4M12 6h2.2M12 10h2.2" />
    </>
  ),
  // A memory module.
  memory: (
    <>
      <rect x="1.8" y="4.5" width="12.4" height="6" rx="1" />
      <path d="M4.5 7v1.5M7 7v1.5M9.5 7v1.5M12 7v1.5M3.5 10.5v2M6 10.5v2M10 10.5v2M12.5 10.5v2" />
    </>
  ),
  // A globe.
  network: (
    <>
      <circle cx="8" cy="8" r="6" />
      <path d="M2 8h12M8 2c1.8 1.7 2.6 3.7 2.6 6S9.8 12.3 8 14c-1.8-1.7-2.6-3.7-2.6-6S6.2 3.7 8 2z" />
    </>
  ),
  download: <path d="M8 2.5v8.5M4.5 7.5 8 11l3.5-3.5M3 13.5h10" />,
  upload: <path d="M8 13V4.5M4.5 8 8 4.5 11.5 8M3 2.5h10" />,
  // Two arrows trading places: memory moved out to disk and back.
  swap: <path d="M3 5.5h9.5M10 3l2.5 2.5L10 8M13 10.5H3.5M6 8l-2.5 2.5L6 13" />,
  // A drive.
  disk: (
    <>
      <rect x="2" y="3.5" width="12" height="9" rx="2" />
      <path d="M2 9.5h12M11 11h.01" />
    </>
  ),
  // Reads and writes.
  diskio: (
    <>
      <path d="M5 2.5v9M2.5 9 5 11.5 7.5 9M11 13.5v-9M8.5 7 11 4.5 13.5 7" />
    </>
  ),
  // A bolt.
  energy: <path d="M9 1.8 3.8 9h4L7 14.2 12.2 7h-4z" />,
  // A battery.
  battery: (
    <>
      <rect x="1.8" y="4.5" width="11" height="7" rx="1.6" />
      <path d="M14.2 7v2M4 6.8v2.4M6.2 6.8v2.4" />
    </>
  ),
  // A thermometer.
  thermal: (
    <>
      <path d="M6.5 9.3V3.5a1.5 1.5 0 0 1 3 0v5.8a3 3 0 1 1-3 0z" />
      <path d="M8 6.5v4.3" />
    </>
  ),
};

export function MetricIcon({ m, size = 14 }: { m: Metric; size?: number }) {
  return (
    <svg
      className="micon"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[m]}
    </svg>
  );
}

/// A label with its icon, e.g. in a card's eyebrow or a chart's title.
export function Labelled({ m, children }: { m: Metric; children: React.ReactNode }) {
  return (
    <span className="labelled">
      <MetricIcon m={m} />
      {children}
    </span>
  );
}
