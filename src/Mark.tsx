// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

/// Tidemark's mark in the sidebar and the menu-bar window: the app icon itself
/// (public/icon.png, copied from src-tauri/icons by `make icons`). The icon carries macOS's
/// margin around its tile, so it is drawn a little larger than `size` to make the tile that size.
export function Mark({ size = 24 }: { size?: number }) {
  const drawn = Math.round(size * 1.24);
  const inset = (drawn - size) / 2;
  return (
    <img
      src="/icon.png"
      width={drawn}
      height={drawn}
      alt=""
      aria-hidden="true"
      draggable={false}
      style={{ flex: "none", margin: -inset, display: "block" }}
    />
  );
}
