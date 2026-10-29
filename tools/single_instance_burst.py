#!/usr/bin/env python3
# Copyright (c) 2026 Wayne Davies
# SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

"""Checks that a burst of launches leaves one Tidemark running (Windows).

    python tools/single_instance_burst.py [--exe PATH] [--count 8] [--wait 30]

Starts the app `--count` times as fast as the shell can (a few hundred milliseconds for eight),
waits `--wait` seconds for the copies to start or hand over (src-tauri/src/single_instance_win.rs),
then counts the Tidemark.exe processes that are the app, not the background helper (`--record`).
Passes when exactly one is left. `--exe` is the build to test (`npx tauri build`'s
src-tauri/target/release/Tidemark.exe); the default is the installed app. A debug build doesn't
take the lock (make dev runs beside the installed app), so test a release build. Quit Tidemark
first, and quit the one left afterwards from its notification-area window.
"""

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path

PS = ["powershell", "-NoProfile", "-Command"]


def app_processes() -> list[int]:
    """The pid of every Tidemark.exe that is the app, not the background helper."""
    out = subprocess.run(
        [
            *PS,
            "Get-CimInstance Win32_Process -Filter \"Name='Tidemark.exe'\" | "
            'ForEach-Object { "$($_.ProcessId)`t$($_.CommandLine)" }',
        ],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    found = []
    for line in out.splitlines():
        pid, _, cmd = line.partition("\t")
        if pid.strip().isdigit() and "--record" not in cmd:
            found.append(int(pid))
    return found


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument(
        "--exe", default=str(Path(os.environ.get("LOCALAPPDATA", "")) / "Tidemark" / "Tidemark.exe")
    )
    ap.add_argument("--count", type=int, default=8)
    ap.add_argument("--wait", type=float, default=30)
    a = ap.parse_args()

    exe = str(Path(a.exe).resolve())
    before = app_processes()
    if before:
        print(f"already running before the burst: {before}; quit it first")
        return 2

    t0 = time.perf_counter()
    for _ in range(a.count):
        subprocess.Popen([exe], close_fds=True)
    print(f"launched {a.count} copies of {exe} in {time.perf_counter() - t0:.2f} s")

    time.sleep(a.wait)
    live = app_processes()
    print(f"after {a.wait:g} s: {len(live)} live copy(ies) {live}")
    ok = len(live) == 1
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
