#!/usr/bin/env python3
# Copyright (c) 2026 Wayne Davies
# SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

"""Writes the files the app's updater reads from a GitHub release, next to the DMG: the built app
packed as Tidemark.app.tar.gz, its signature (.sig), and latest.json, which names the version and
where to download it. The app checks releases/latest/download/latest.json (tauri.conf.json,
plugins.updater) and only installs a package whose signature matches the public key there.

    python3 tools/update_files.py                     # after make app; make dmg runs it
    python3 tools/update_files.py --notes "What's new"

Signs with the private key in ~/.tauri/waynetd777-updater.key, or TAURI_SIGNING_PRIVATE_KEY_PATH.
Keep that key safe: without it no installed copy will accept another update.
"""

import argparse
import datetime
import json
import os
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONF = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())
NAME, VERSION = CONF["productName"], CONF["version"]
APP = ROOT / f"src-tauri/target/release/bundle/macos/{NAME}.app"
OUT = ROOT / "dist-release"
REPO = "waynetd777/tidemark"
KEY = Path(
    os.environ.get("TAURI_SIGNING_PRIVATE_KEY_PATH", Path.home() / ".tauri/waynetd777-updater.key")
)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--notes", default="", help="what's new, shown in the app's update prompt")
    notes = ap.parse_args().notes
    if not APP.exists():
        sys.exit(f"update_files: no {APP.relative_to(ROOT)}; run make app first")
    if not KEY.exists():
        sys.exit(f"update_files: no signing key at {KEY}")
    OUT.mkdir(parents=True, exist_ok=True)
    archive = OUT / f"{NAME.replace(' ', '-')}.app.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(APP, arcname=APP.name)
    password = os.environ.get("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", "")
    env = {
        **os.environ,
        "TAURI_SIGNING_PRIVATE_KEY_PATH": str(KEY),
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": password,
    }
    env.pop("TAURI_SIGNING_PRIVATE_KEY", None)
    subprocess.run(
        ["npx", "tauri", "signer", "sign", str(archive)],
        cwd=ROOT,
        env=env,
        check=True,
        stdout=subprocess.DEVNULL,
    )
    signature = Path(f"{archive}.sig").read_text().strip()
    latest = {
        "version": VERSION,
        "notes": notes,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            "darwin-aarch64": {
                "signature": signature,
                "url": f"https://github.com/{REPO}/releases/download/v{VERSION}/{archive.name}",
            }
        },
    }
    (OUT / "latest.json").write_text(json.dumps(latest, indent=2) + "\n")
    for f in (archive, Path(f"{archive}.sig"), OUT / "latest.json"):
        print(f.relative_to(ROOT))


if __name__ == "__main__":
    main()
