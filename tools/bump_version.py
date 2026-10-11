#!/usr/bin/env python3
# Copyright (c) 2026 Wayne Davies
# SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

"""Bumps the app's version for a release build: the last number of tauri.conf.json's version
(2.1.5 → 2.1.6), written to package.json and src-tauri/Cargo.toml (and both lock files) too, so
they agree. Prints the new version. A version with no release yet (no v<version> tag on origin,
or locally when origin can't be reached) is kept, so local builds don't use up numbers.

    python3 tools/bump_version.py            # 2.1.5 → 2.1.6, if v2.1.5 was released
    python3 tools/bump_version.py 2.2.0      # set it (a minor or major step is chosen by hand)

`make release` runs it before the release build (tools/release.py); other builds keep the version.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONF, PACKAGE, NPMLOCK, CARGO, LOCK = (
    ROOT / p
    for p in (
        "src-tauri/tauri.conf.json",
        "package.json",
        "package-lock.json",
        "src-tauri/Cargo.toml",
        "src-tauri/Cargo.lock",
    )
)


def replace(path, pattern, new, count=1):
    text = path.read_text()
    out, n = re.subn(pattern, new, text, count=count, flags=re.M)
    if n != count:
        sys.exit(f"bump_version: no version found in {path.relative_to(ROOT)}")
    path.write_text(out)


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=False)


def released(version):
    tag = f"v{version}"
    remote = git("ls-remote", "--tags", "origin", tag)
    if remote.returncode == 0:
        return bool(remote.stdout.strip())
    return bool(git("tag", "-l", tag).stdout.strip())


def main():
    old = json.loads(CONF.read_text())["version"]
    if len(sys.argv) > 1:
        new = sys.argv[1]
        if not re.fullmatch(r"\d+\.\d+\.\d+", new):
            sys.exit(f"bump_version: {new!r} is not major.minor.patch")
    elif not released(old):
        new = old
    else:
        major, minor, patch = (int(x) for x in old.split("."))
        new = f"{major}.{minor}.{patch + 1}"
    replace(CONF, r'^(  "version": )"[^"]+"', rf'\1"{new}"')
    replace(PACKAGE, r'^(  "version": )"[^"]+"', rf'\1"{new}"')
    if NPMLOCK.exists():  # the lock's own entry, and its root package's
        replace(NPMLOCK, r'^(  "name": "tidemark",\n  "version": )"[^"]+"', rf'\1"{new}"')
        replace(NPMLOCK, r'^(      "name": "tidemark",\n      "version": )"[^"]+"', rf'\1"{new}"')
    replace(CARGO, r'^(version = )"[^"]+"', rf'\1"{new}"')
    if LOCK.exists():
        replace(LOCK, r'^(name = "tidemark"\nversion = )"[^"]+"', rf'\1"{new}"')
    print(new)


if __name__ == "__main__":
    main()
