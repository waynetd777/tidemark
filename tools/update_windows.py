#!/usr/bin/env python3
# Copyright (c) 2026 Wayne Davies
# SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

"""Adds the Windows installer to a GitHub release the Mac's make release published: signs the NSIS
installer with the updater key, uploads it and its signature, and adds windows-x86_64 to the
release's latest.json, which installed copies read (tauri.conf.json, plugins.updater). Run by
.github/workflows/windows-release.yml on Windows, after `npx tauri build`.

    python tools/update_windows.py v2.2.0

Signs with TAURI_SIGNING_PRIVATE_KEY (the key's contents, a repository secret) and
TAURI_SIGNING_PRIVATE_KEY_PASSWORD; uploads with gh, which needs GH_TOKEN.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Python's default on Windows is the ANSI code page: every text file is read as UTF-8.
CONF = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
NAME, VERSION = CONF["productName"], CONF["version"]
BUNDLE = ROOT / "src-tauri/target/release/bundle/nsis"
REPO = "waynetd777/tidemark"
# One name for every version, so the README's link to the latest release's installer never changes.
ASSET = f"{NAME.replace(' ', '-')}_x64-setup.exe"


def run(*cmd, **kw):
    return subprocess.run(cmd, cwd=ROOT, check=True, **kw)


def main():
    tag = sys.argv[1] if len(sys.argv) > 1 else f"v{VERSION}"
    if tag != f"v{VERSION}":
        sys.exit(f"update_windows: the release is {tag} but this checkout is {VERSION}")
    built = BUNDLE / f"{NAME}_{VERSION}_x64-setup.exe"
    if not built.exists():
        sys.exit(f"update_windows: no {built.relative_to(ROOT)}; run npx tauri build first")
    out = BUNDLE / ASSET
    shutil.copyfile(built, out)
    # npx is npx.cmd on Windows, which only a shell runs.
    win = sys.platform == "win32"
    run("npx", "tauri", "signer", "sign", str(out), shell=win, stdout=subprocess.DEVNULL)
    sig = Path(f"{out}.sig")
    latest = BUNDLE / "latest.json"
    run(
        "gh", "release", "download", tag, "--repo", REPO,
        "--pattern", "latest.json", "--dir", str(BUNDLE), "--clobber",
    )  # fmt: skip
    data = json.loads(latest.read_text(encoding="utf-8"))
    data["platforms"]["windows-x86_64"] = {
        "signature": sig.read_text(encoding="utf-8").strip(),
        "url": f"https://github.com/{REPO}/releases/download/{tag}/{ASSET}",
    }
    latest.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    files = [str(out), str(sig), str(latest)]
    run("gh", "release", "upload", tag, "--repo", REPO, "--clobber", *files)
    print(f"added {ASSET} to {tag}")


if __name__ == "__main__":
    main()
