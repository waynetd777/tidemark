#!/usr/bin/env python3
# Copyright (c) 2026 Wayne Davies
# SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

"""Retake the docs screenshots from demo data: docs/images/<scene>-<theme>.png.

After Keepr's tools/screenshots.py. Makes a demo in .demo/ (gitignored): two weeks of history
(minutes with the busiest apps, nights asleep, incidents, crashes), a scan of a made-up disk with
a week's growth, a year of daily disk totals, and live readings. Then it launches the dev build
once per scene and theme with the scene in TIDEMARK_SCENE: the app records and scans nothing and
shows only the demo. Its window is invisible and takes no focus, so nothing flashes on screen;
the app saves its webview's snapshot to TIDEMARK_SNAPSHOT, and the window's buttons and rounded
corners are drawn back on here. Nothing outside .demo/ is read or written.

    python3 tools/screenshots.py                 # every scene, light and dark
    python3 tools/screenshots.py performance     # one scene
    python3 tools/screenshots.py --theme dark    # one theme
    python3 tools/screenshots.py -j 1            # one at a time (default: 3 side by side)

Needs the Vite dev server (started here if it isn't running) and Pillow.
"""

import argparse
import concurrent.futures
import io
import json
import math
import os
import pathlib
import queue
import random
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request
from datetime import datetime, timedelta

from PIL import Image, ImageCms, ImageDraw, ImageStat

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEMO = ROOT / ".demo"
OUT = ROOT / "docs" / "images"
BIN = ROOT / "src-tauri" / "target" / "debug" / "Tidemark"
DEV_URL = "http://localhost:1420"
WIDTH = 1400
SETTLE = 6.0
GB = 1e9
HOME_LABEL = "/Users/alex"
MEM_TOTAL = 24 * GB

SCENES = [
    {"name": "now", "screen": "now", "height": 1290},
    {"name": "performance", "screen": "performance", "height": 1000},
    {"name": "disk-map", "screen": "disk", "height": 900},
    {"name": "trends", "screen": "trends", "height": 1180},
    {"name": "settings", "screen": "settings", "height": 640},
    {"name": "menu-bar", "tray": True},
]

APPS = ["Safari", "Xcode", "Mail", "Slack", "Music", "Photos", "Finder", "Terminal"]


def core_counts():
    """This Mac's performance and efficiency cores, so the demo's cores match the kinds the app
    reads from the device tree; (0, 0) where it doesn't say."""

    def n(key):
        out = subprocess.run(
            ["sysctl", "-n", key], capture_output=True, text=True, check=False
        ).stdout
        return int(out) if out.strip().isdigit() else 0

    return n("hw.perflevel0.logicalcpu"), n("hw.perflevel1.logicalcpu")


P_CORES, E_CORES = core_counts()


def split(cpu):
    """The CPU's average as its efficiency and performance cores' averages: macOS puts the
    background work on the efficiency cores, so they run busier while the Mac is quiet."""
    if not P_CORES or not E_CORES:
        return None, None
    e = min(100.0, cpu * 1.6)
    p = max(0.0, (cpu * (P_CORES + E_CORES) - e * E_CORES) / P_CORES)
    return min(100.0, p), e


# ---- the demo ----


def fnv(s):
    """cache.rs's file name for a root's scan: FNV-1a, 64-bit."""
    h = 0xCBF29CE484222325
    for b in s.encode():
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def node(name, size, kids=(), files=0, cloud=False):
    kids = list(kids)
    loose = max(0, size - sum(k["s"] for k in kids))
    n = {
        "n": name,
        "s": int(size),
        "a": int(size * (3 if cloud else 1.02)),
        "f": files,
        "ls": int(loose),
        "la": int(loose * 1.02),
        "i": files + sum(k["i"] + 1 for k in kids),
        "k": kids,
    }
    if cloud:
        n["c"] = True
    return n


def demo_tree(filler, grown):
    """A made-up disk: `grown` scales the folders that grew (1 now, 0 a week ago); `filler` is
    what makes the map's total look like this Mac's disk, the same in every scan."""

    def g(base, extra):
        return (base + extra * grown) * GB

    alex = node(
        "alex",
        0,
        [
            node(
                "Library",
                0,
                [
                    node(
                        "Caches", g(5.4, 2.1), [node("com.apple.Safari", 1.8 * GB, files=900)], 4200
                    ),
                    node(
                        "Application Support",
                        18.6 * GB,
                        [node("Xcode", 7.2 * GB, files=3100), node("Slack", 2.4 * GB, files=800)],
                        2400,
                    ),
                    node(
                        "Mobile Documents",
                        2.1 * GB,
                        [node("com~apple~CloudDocs", 2.0 * GB, files=600, cloud=True)],
                        12,
                        cloud=True,
                    ),
                    node(
                        "Containers", 9.8 * GB, [node("com.apple.mail", 4.6 * GB, files=52000)], 300
                    ),
                ],
                800,
            ),
            node(
                "Movies",
                0,
                [node("Exports", g(4.1, 8.4), files=24), node("Footage", 31.0 * GB, files=180)],
                6,
            ),
            node(
                "Projects",
                0,
                [
                    node(
                        "atlas",
                        0,
                        [node("target", g(5.2, 1.6), files=8200), node("src", 0.4 * GB, files=900)],
                        40,
                    ),
                    node("harbour", 6.3 * GB, [node("node_modules", 3.9 * GB, files=41000)], 120),
                    node("notes-site", 1.2 * GB, files=600),
                ],
                30,
            ),
            node("Pictures", 0, [node("Photos Library.photoslibrary", 48.5 * GB, files=28000)], 40),
            node("Downloads", g(6.0, 0.9), files=320),
            node("Documents", 7.4 * GB, files=2100),
            node("Music", 14.2 * GB, files=4800),
        ],
        40,
    )
    for k in alex["k"]:
        if k["s"] == 0:
            k["s"] = sum(c["s"] for c in k["k"]) + k["ls"]
    alex["s"] = sum(k["s"] for k in alex["k"])
    users = node("Users", alex["s"], [alex])
    apps = node(
        "Applications",
        34.7 * GB,
        [
            node("Xcode.app", 11.2 * GB, files=1),
            node("Logic Pro.app", 6.8 * GB, files=1),
            node("Final Cut Pro.app", 5.9 * GB, files=1),
        ],
        60,
    )
    system = node(
        "System",
        25.4 * GB,
        [node("Library", 24.4 * GB, [node("AssetsV2", 13.0 * GB, files=900)], 2000)],
    )
    library = node("Library", 6.2 * GB, [node("Developer", 4.4 * GB, files=4200)], 300)
    private = node("private", 8.4 * GB, [node("var", 7.9 * GB, files=12000)])
    opt = node("opt", 4.6 * GB, [node("homebrew", 4.5 * GB, files=38000)])
    tops = [users, apps, system, library, private, opt]
    # Fill to the disk's size, so the map's total looks real.
    if filler > 0:
        users["k"][0]["k"].append(node("Virtual Machines", filler, files=4))
        users["k"][0]["s"] += int(filler)
        users["s"] += int(filler)
    root = node("Macintosh HD", sum(t["s"] for t in tops), tops)
    return root


def folders(tree, path="/", depth=0, out=None):
    """The scan's folders as history.rs keeps them: 100 MB or more, 8 levels at most."""
    out = [] if out is None else out
    if tree["s"] < 100e6:
        return out
    out.append((path, tree["s"]))
    if depth >= 8:
        return out
    for k in tree["k"]:
        folders(k, (path if path != "/" else "") + "/" + k["n"], depth + 1, out)
    return out


SCHEMA = """
CREATE TABLE perf (
  ts INTEGER PRIMARY KEY, cpu REAL, cpu_max REAL, mem REAL, mem_max REAL, mem_total REAL,
  swap REAL, swap_max REAL, pressure INTEGER, disk_r REAL, disk_w REAL, disk_max REAL,
  net_in REAL, net_out REAL, net_max REAL, thermal INTEGER, power REAL, power_max REAL,
  battery REAL, on_ac INTEGER, gpu REAL, gpu_max REAL, mem_app REAL, mem_wired REAL,
  mem_comp REAL, net_in_max REAL, net_out_max REAL, cpu_p REAL, cpu_e REAL);
CREATE TABLE procs (ts INTEGER, app TEXT, cpu REAL, mem REAL, energy REAL, PRIMARY KEY (ts, app));
CREATE TABLE incidents (
  id INTEGER PRIMARY KEY, kind TEXT, started INTEGER, ended INTEGER, peak REAL, peak_at INTEGER,
  snapshot TEXT);
CREATE TABLE disk_used (ts INTEGER PRIMARY KEY, used REAL, total REAL);
CREATE TABLE disk_snap (ts INTEGER, root TEXT, path TEXT, size REAL, PRIMARY KEY (ts, path));
CREATE TABLE crashes (ts INTEGER, app TEXT, kind TEXT, file TEXT PRIMARY KEY);
CREATE TABLE sleeps (
  started INTEGER PRIMARY KEY, ended INTEGER, battery_before REAL, battery_after REAL);
CREATE TABLE disk_daily (day TEXT PRIMARY KEY, used REAL, used_max REAL, total REAL, top TEXT);
"""


def make_demo():
    r = random.Random(7)
    shutil.rmtree(DEMO, ignore_errors=True)
    data = DEMO / "data"
    data.mkdir(parents=True)
    (DEMO / "home" / "Library" / "Safari").mkdir(parents=True)
    st = os.statvfs("/")
    total = st.f_blocks * st.f_frsize
    disk_used_now = total - st.f_bavail * st.f_frsize
    now = int(time.time()) // 60 * 60
    db = sqlite3.connect(data / "history.sqlite")
    db.executescript(SCHEMA)

    # Two weeks of minutes: weekday work, quiet weekends, nights asleep.
    incidents = [
        (now - 4 * 86400 + 14 * 3600, "cpu"),
        (now - 2 * 86400 + 10 * 3600 + 2400, "pressure"),
    ]
    battery = 92.0
    t = now - 14 * 86400
    asleep_from = None
    perf, procs = [], []
    sm = {"rd": 1e6, "wr": 1.5e6, "nin": 0.5e6, "nout": 0.1e6}
    while t <= now - 60:
        d = datetime.fromtimestamp(t)
        # Asleep at night, except the last few hours: whenever the shots are taken, the Mac is
        # awake and has been for a while.
        night = (d.hour < 7 or (d.hour == 23 and d.minute >= 40)) and t < now - 4 * 3600
        if night:
            if asleep_from is None:
                asleep_from = (t, battery)
            t += 60
            continue
        if asleep_from is not None:
            battery = max(15.0, asleep_from[1] - r.uniform(2, 5))
            db.execute(
                "INSERT INTO sleeps VALUES (?,?,?,?)", (asleep_from[0], t, asleep_from[1], battery)
            )
            asleep_from = None
        work = d.weekday() < 5 and (9 <= d.hour < 12 or 14 <= d.hour < 18)
        cpu = r.uniform(4, 10) + (r.uniform(10, 26) if work else 0)
        mem = 12.5 * GB + (3.0 * GB if work else 0) + r.uniform(-0.4, 0.4) * GB
        swap, pressure = 0.6 * GB, 1
        for at, kind in incidents:
            if 0 <= t - at < 300:
                if kind == "cpu":
                    cpu = r.uniform(91, 98)
                else:
                    mem, swap, pressure = 22.1 * GB, 4.2 * GB, 4
        on_ac = d.hour >= 15 and d.weekday() < 5
        battery = min(100.0, battery + 0.3) if on_ac else max(18.0, battery - 0.04 - cpu / 3000)
        power = 6 + cpu * 0.28 + r.uniform(-1, 1)
        gpu = r.uniform(2, 8) + (r.uniform(20, 45) if d.hour == 20 and d.weekday() >= 4 else 0)
        # Each minute eases toward a new random value, as a real Mac's readings drift.
        sm["rd"] = 0.8 * sm["rd"] + 0.2 * r.uniform(0.2, 3) * 1e6
        sm["wr"] = 0.8 * sm["wr"] + 0.2 * r.uniform(0.3, 4) * 1e6
        sm["nin"] = 0.85 * sm["nin"] + 0.15 * r.uniform(0.05, 2.5 if work else 0.8) * 1e6
        sm["nout"] = 0.85 * sm["nout"] + 0.15 * r.uniform(0.02, 0.5) * 1e6
        rd, wr, nin, nout = sm["rd"], sm["wr"], sm["nin"], sm["nout"]
        if r.random() < 0.004:
            rd *= 40
        perf.append(
            (
                t,
                cpu,
                min(100, cpu * 1.35 + r.uniform(0, 8)),
                mem,
                mem * 1.03,
                MEM_TOTAL,
                swap,
                swap * 1.05,
                pressure,
                rd,
                wr,
                (rd + wr) * 1.4,
                nin,
                nout,
                (nin + nout) * 1.5,
                0,
                power,
                power * 1.3,
                battery,
                int(on_ac),
                gpu,
                min(100, gpu * 1.4),
                mem * 0.62,
                3.2 * GB,
                mem * 0.38 - 3.2 * GB + 2.1 * GB,
                nin * 1.6,
                nout * 1.6,
                *split(cpu),
            )
        )
        weights = [0.32, 0.26 if work else 0.04, 0.08, 0.1, 0.06, 0.05, 0.03, 0.05]
        mems = [3.4, 2.9 if work else 0.6, 0.9, 0.8, 0.4, 0.7, 0.2, 0.1]
        for a, w, m in zip(APPS, weights, mems, strict=True):
            procs.append((t, a, cpu * w, m * GB, power * w * 0.5))
        t += 60
    db.executemany("INSERT INTO perf VALUES (" + ",".join("?" * 29) + ")", perf)
    db.executemany("INSERT INTO procs VALUES (?,?,?,?,?)", procs)
    snap_cpu = json.dumps(
        [
            {"app": "Xcode", "cpu": 71.0, "mem": 3.1 * GB, "energy": 9.0},
            {"app": "Safari", "cpu": 9.0, "mem": 3.4 * GB, "energy": 2.0},
        ]
    )
    snap_mem = json.dumps(
        [
            {"app": "Photos", "cpu": 4.0, "mem": 8.2 * GB, "energy": 1.0},
            {"app": "Safari", "cpu": 7.0, "mem": 6.1 * GB, "energy": 2.0},
        ]
    )
    db.execute(
        "INSERT INTO incidents VALUES (1,'cpu',?,?,96.0,?,?)",
        (incidents[0][0], incidents[0][0] + 240, incidents[0][0] + 60, snap_cpu),
    )
    db.execute(
        "INSERT INTO incidents VALUES (2,'pressure',?,?,?,?,?)",
        (incidents[1][0], incidents[1][0] + 600, 22.1 * GB, incidents[1][0] + 120, snap_mem),
    )
    for i, (app, kind, ago) in enumerate(
        [("Music", "crash", 1.2), ("Music", "crash", 3.6), ("Photos", "hang", 2.3)]
    ):
        db.execute(
            "INSERT INTO crashes VALUES (?,?,?,?)",
            (now - int(ago * 86400), app, kind, f"demo-{i}.ips"),
        )

    # The disk: hourly used space, two scans a week apart, and a year of days.
    filler = int(max(0, disk_used_now * 0.93 - demo_tree(0, 1.0)["s"]))
    tree_now = demo_tree(filler, 1.0)
    tree_then = demo_tree(filler, 0.0)
    for h in range(14 * 24):
        ts = now - (14 * 24 - h) * 3600
        db.execute(
            "INSERT INTO disk_used VALUES (?,?,?)",
            (
                ts,
                disk_used_now - (13 * GB) * (1 - h / (14 * 24)) + r.uniform(-0.2, 0.2) * GB,
                total,
            ),
        )
    scans = [(now - 6 * 86400, tree_then), (now - 3000, tree_now)]
    for ts, tr in scans:
        db.executemany(
            "INSERT INTO disk_snap VALUES (?,?,?,?)", [(ts, "/", p, s) for p, s in folders(tr)]
        )

    def top_of(tree):
        return json.dumps({p: s for p, s in folders(tree) if p != "/" and p.count("/") <= 4})

    for day in range(365):
        dt = datetime.fromtimestamp(now) - timedelta(days=364 - day)
        used = disk_used_now - 60 * GB * (1 - day / 364) + 4 * GB * math.sin(day / 9)
        # The folders that grew this week grew over the year too, so longer ranges show them.
        top = top_of(demo_tree(filler, day / 364))
        db.execute(
            "INSERT INTO disk_daily VALUES (?,?,?,?,?)",
            (dt.strftime("%Y-%m-%d"), used, used + 0.6 * GB, total, top),
        )
    db.commit()
    db.close()

    finished = datetime.fromtimestamp(now - 3000).astimezone().isoformat()
    scan = {
        "root": "/",
        "root_name": "Macintosh HD",
        "finished": finished,
        "duration_ms": 31000,
        "items": tree_now["i"],
        "size": tree_now["s"],
        "apparent": tree_now["a"],
        "denied": 0,
        "tree": tree_now,
    }
    (data / "scans").mkdir()
    (data / "scans" / f"{fnv('/')}.json").write_text(json.dumps(scan))
    (data / "root.txt").write_text("/")
    recent = [
        {
            "path": "/",
            "name": "Macintosh HD",
            "size": tree_now["s"],
            "items": tree_now["i"],
            "when": finished,
        },
        {
            "path": f"{HOME_LABEL}/Projects",
            "name": "Projects",
            "size": 14.7 * GB,
            "items": 51000,
            "when": finished,
        },
    ]
    (data / "recent.json").write_text(json.dumps(recent))
    (data / "settings.json").write_text(
        json.dumps({"history_days": 14, "scan": "daily", "theme": "system"})
    )


def write_live(data):
    """The live readings the app follows, fresh (it ignores a stale file)."""
    r = random.Random(11)
    now_ms = int(time.time() * 1000)
    samples = []
    nin, nout = 1.2e6, 0.25e6
    for i in range(450):
        nin = 0.9 * nin + 0.1 * r.uniform(0.3, 2.4) * 1e6
        nout = 0.9 * nout + 0.1 * r.uniform(0.08, 0.45) * 1e6
        cpu = 14 + 8 * math.sin(i / 40) + r.uniform(-4, 4)
        samples.append(
            {
                "t": now_ms - (449 - i) * 2000,
                "cpu": cpu,
                "mem": 14.2 * GB + r.uniform(-0.1, 0.1) * GB,
                "swap": 0.6 * GB,
                "pressure": 1,
                "disk_r": r.uniform(0.3, 2) * 1e6,
                "disk_w": r.uniform(0.4, 3) * 1e6,
                "net_in": nin,
                "net_out": nout,
                "thermal": 0,
                "power": 7 + cpu * 0.25,
                "battery": 72.0,
                "gpu": r.uniform(3, 9),
                "mem_parts": {"app": 8.9 * GB, "wired": 3.2 * GB, "compressed": 2.1 * GB},
                "on_ac": False,
                "cpu_p": split(cpu)[0],
                "cpu_e": split(cpu)[1],
            }
        )
    # Each core's use now, around the last reading's split: efficiency cores first, as the device
    # tree numbers them.
    p, e = split(samples[-1]["cpu"])
    cores = [max(0.0, e * r.uniform(0.4, 1.6)) for _ in range(E_CORES)] + [
        max(0.0, p * r.uniform(0.2, 2.2)) for _ in range(P_CORES)
    ]
    apps = [
        {"app": a, "cpu": c, "mem": m * GB, "energy": e}
        for a, c, m, e in [
            ("Safari", 5.1, 3.4, 1.9),
            ("Xcode", 3.2, 2.9, 1.4),
            ("Slack", 1.4, 0.8, 0.4),
            ("Mail", 0.9, 0.9, 0.3),
            ("Music", 0.8, 0.4, 0.3),
            ("Photos", 0.6, 0.7, 0.2),
            ("Finder", 0.3, 0.2, 0.1),
            ("Terminal", 0.2, 0.1, 0.1),
        ]
    ]
    live = {
        "samples": samples,
        "apps": apps,
        "processes": 612,
        "mem_total": MEM_TOTAL,
        "battery": {
            "percent": 72.0,
            "charging": False,
            "on_ac": False,
            "fully_charged": False,
            "watts": 10.4,
            "minutes_left": 312,
            "cycles": 143,
            "health": 96.0,
        },
        "paused_until": None,
        "open": {},
        "cores": cores,
    }
    (data / "live.json").write_text(json.dumps(live))


# ---- taking the shots ----


def to_srgb(im):
    icc = im.info.get("icc_profile")
    if icc:
        alpha = im.getchannel("A") if im.mode == "RGBA" else None
        src = ImageCms.ImageCmsProfile(io.BytesIO(icc))
        im = ImageCms.profileToProfile(
            im.convert("RGB"),
            src,
            ImageCms.createProfile("sRGB"),
            renderingIntent=ImageCms.Intent.PERCEPTUAL,
        )
        if alpha:
            im.putalpha(alpha)
    im.info.pop("icc_profile", None)
    return im


def dev_server():
    try:
        urllib.request.urlopen(DEV_URL, timeout=1)
        return None
    except OSError:
        pass
    p = subprocess.Popen(
        ["npx", "vite", "--port", "1420", "--strictPort"],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    for _ in range(60):
        time.sleep(0.5)
        try:
            urllib.request.urlopen(DEV_URL, timeout=1)
            return p
        except OSError:
            pass
    sys.exit("the Vite dev server didn't start")


def capture(scene, theme, data):
    """Launches the app on the scene, unseen, and returns its webview's snapshot, or None."""
    sc = {k: v for k, v in scene.items() if k != "name"}
    sc["theme"] = theme
    write_live(data)
    with tempfile.TemporaryDirectory() as tmp:
        shot = pathlib.Path(tmp) / "shot.tiff"
        app = subprocess.Popen(
            [str(BIN)],
            cwd=ROOT / "src-tauri",
            env={
                **os.environ,
                "HOME": str(DEMO / "home"),
                "TIDEMARK_DATA": str(data),
                "TIDEMARK_SCENE": json.dumps(sc),
                "TIDEMARK_SCENE_HOME": HOME_LABEL,
                "TIDEMARK_SNAPSHOT": str(shot),
                "TIDEMARK_SNAPSHOT_AFTER": str(SETTLE),
            },
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            end = time.time() + SETTLE + 30
            while not shot.exists() and app.poll() is None and time.time() < end:
                time.sleep(0.2)
            if not shot.exists():
                return None
            time.sleep(0.3)
            im = Image.open(shot)
            im.load()
            return to_srgb(im.convert("RGBA") if im.mode not in ("RGB", "RGBA") else im)
        finally:
            app.send_signal(signal.SIGKILL)
            app.wait()


def chrome(im, theme):
    """Draws back what a webview snapshot leaves out: the window's buttons and rounded corners."""
    k = 4
    w, h = im.size
    over = Image.new("RGBA", (w * k, h * k))
    d = ImageDraw.Draw(over)
    fill, rim = (
        ((214, 214, 218), (200, 200, 205)) if theme == "light" else ((96, 97, 100), (110, 111, 114))
    )
    for cx in (23.5, 46, 68.5):
        cy, r = 19, 7
        d.ellipse(
            [(cx - r) * k, (cy - r) * k, (cx + r) * k, (cy + r) * k],
            fill=(*fill, 255),
            outline=(*rim, 255),
            width=k,
        )
    im = Image.alpha_composite(im.convert("RGBA"), over.resize((w, h), Image.LANCZOS))
    mask = Image.new("L", (w * k, h * k))
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, w * k - 1, h * k - 1], radius=16 * k, fill=255)
    im.putalpha(mask.resize((w, h), Image.LANCZOS))
    return im


def blank(im):
    return ImageStat.Stat(im.convert("L")).stddev[0] < 1


def shoot(scene, theme, data):
    im = None
    for _ in range(3):
        im = capture(scene, theme, data)
        if im is not None and not blank(im):
            break
    else:
        print(f"  {scene['name']} {theme}: " + ("no window" if im is None else "blank"))
        return False
    if not scene.get("tray"):
        im = im.resize((WIDTH, round(im.height * WIDTH / im.width)), Image.LANCZOS)
        im = chrome(im, theme)
    else:
        # The menu-bar window: its own panel, without the transparent margin below it.
        bbox = im.getchannel("A").getbbox() if im.mode == "RGBA" else None
        if bbox:
            im = im.crop(bbox)
    out = OUT / f"{scene['name']}-{theme}.png"
    im.save(out, optimize=True)
    print(f"  {out.relative_to(ROOT)}  {im.width}x{im.height}")
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="*")
    ap.add_argument("--theme", choices=["light", "dark"], action="append")
    ap.add_argument(
        "-j", type=int, default=3, metavar="N", help="apps to run side by side (default 3)"
    )
    a = ap.parse_args()
    scenes = [s for s in SCENES if not a.names or s["name"] in a.names]
    themes = a.theme or ["light", "dark"]
    OUT.mkdir(parents=True, exist_ok=True)
    server = dev_server()
    try:
        subprocess.run(["cargo", "build"], cwd=ROOT / "src-tauri", check=True)
        make_demo()
        with tempfile.TemporaryDirectory() as tmp:
            jobs = [(s, t) for s in scenes for t in themes]
            n = max(1, min(a.j, len(jobs)))
            free = queue.Queue()
            for i in range(n):
                d = pathlib.Path(tmp) / f"data-{i}"
                shutil.copytree(DEMO / "data", d)
                free.put(d)

            def one(job):
                d = free.get()
                try:
                    return shoot(*job, d)
                finally:
                    free.put(d)

            with concurrent.futures.ThreadPoolExecutor(n) as pool:
                ok = all(list(pool.map(one, jobs)))
    finally:
        if server:
            server.terminate()
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
