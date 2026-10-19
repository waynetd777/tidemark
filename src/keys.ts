// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// The platform the page runs on, in the words and keys it uses: Tidemark runs on macOS and on
// Windows, and says Finder or File Explorer, menu bar or notification area, ⌘ or Ctrl, as each
// does.

export const isMac = navigator.userAgent.includes("Mac");

/** Whether the shortcut key (⌘ or Ctrl) is held. */
export const cmdKey = (e: { metaKey: boolean; ctrlKey: boolean }) =>
  isMac ? e.metaKey : e.ctrlKey;

const MODS: Record<string, string> = { "⌃": "Ctrl", "⌘": "Ctrl", "⌥": "Alt", "⇧": "Shift" };
const ORDER = ["Ctrl", "Alt", "Shift"];
const NAMES: Record<string, string> = { "+": "Plus", "−": "Minus", "↑": "Up", "↓": "Down" };

/** Shortcuts as they're written on macOS ("⇧⌘R", "⌘-click", "⌘↑"), in Windows' words there
 *  ("Ctrl+Shift+R", "Ctrl-click", "Ctrl+Up"); unchanged on macOS. */
export function shortcut(s: string): string {
  if (isMac) return s;
  return s
    .replace(/([⌃⌥⇧⌘]+)([^\s)·⌃⌥⇧⌘-])?/g, (_, mods: string, key?: string) => {
      const names = ORDER.filter((n) => [...mods].some((m) => MODS[m] === n));
      return [...names, ...(key ? [NAMES[key] ?? key.toUpperCase()] : [])].join("+");
    })
    .replace(/[↩⏎]/g, "Enter");
}

/** What the app's words call the computer and its parts, on macOS and on Windows. */
export const OS = isMac ? "macOS" : "Windows";
export const MACHINE = isMac ? "Mac" : "PC";
export const FILES_APP = isMac ? "Finder" : "File Explorer";
export const MONITOR_APP = isMac ? "Activity Monitor" : "Task Manager";
export const MENU_BAR = isMac ? "menu bar" : "notification area";

/** Where the disk's used space goes that no folder walk reaches. */
export const UNREACHED = isMac
  ? "local Time Machine snapshots, purgeable caches, swap, the recovery volume, and folders macOS denied access to"
  : "System Restore points and shadow copies, the file system's own records, and folders Windows denied access to";

/** What the folders the scan couldn't read mostly are. */
export const DENIED = isMac
  ? "Mostly system folders owned by root (e.g. /private/var/folders, /System/Library/Templates). Full Disk Access opens your own protected folders, not those."
  : "Mostly folders only an administrator can open: other accounts' folders, System Volume Information and parts of the Windows folder.";

/** Starting a session: macOS logs in, Windows signs in. */
export const LOG_IN = isMac ? "log in" : "sign in";

/** Where the app keeps its history and scans (src-tauri/src/cache.rs dir). */
export const DATA_DIR = isMac
  ? "~/Library/Application Support/Tidemark"
  : "%LOCALAPPDATA%\\com.wayned.tidemark";
