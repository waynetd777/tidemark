// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

/// Tidemark's mark, as in the sidebar and the menu-bar window: a tide-coloured tile with the
/// dashed line the water reached above a wave. The app icon itself is drawn by tools/make_icons.py.
export function Mark({ size = 24 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" style={{ flex: "none" }}>
      <rect width="24" height="24" rx="6.5" style={{ fill: "var(--tide)" }} />
      <path
        d="M5.5 9.5h13"
        fill="none"
        stroke="#fff"
        strokeWidth="1.6"
        strokeDasharray="2 2.2"
        strokeLinecap="round"
      />
      <path
        d="M4.5 15.5c2-2.2 3.6-2.2 5 0s3 2.2 5 0 3.2-2.2 5 0"
        fill="none"
        stroke="#fff"
        strokeWidth="1.9"
        strokeLinecap="round"
      />
    </svg>
  );
}
