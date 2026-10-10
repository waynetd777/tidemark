#!/usr/bin/env python3
# Copyright (c) 2026 Wayne Davies
# SPDX-License-Identifier: MIT (see LICENSE at the repository root)

"""Draw the app's artwork: design/icon.png (the Dock icon source) and the menu-bar template images
src-tauri/icons/tray@2x.png and tray-alert@2x.png.

The icon is a tidemark: deep water in a dark teal tile, a calm wave across its lower half, a faint
older mark, and the dashed line the water once reached. Drawn with PIL rather than exported from
an SVG because no SVG rasteriser is installed here, so this file is the source of truth for the
artwork. The geometry follows the design's SVG (viewBox 100..924, the tile's own coordinates).

Lessons learned the hard way:
  * the tile needs real transparent corners (qlmanage flattens on white, PIL does not);
  * the tray image is a TEMPLATE: black plus alpha only, macOS tints it, so shape is the message;
  * supersample and downsample for smooth edges.

    python3 tools/make_icons.py            # then: npx tauri icon design/icon.png
    make icons                             # does both
"""

import itertools
import math
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
S = 1024
SS = 4

DEEP = (16, 48, 58)  # #10303a
WAVE = (27, 157, 179)  # #1b9db3
WAVE_LOW = (19, 129, 154)  # #13819a
OLD_MARK = (92, 192, 214)  # #5cc0d6, drawn faint
MARK = (244, 241, 234)  # #f4f1ea


def px(v):
    return round(v * SS)


def wave(d, base, amp, fill, x0=100, x1=924, period=412, scale=px):
    """A filled sine-like wave, crest first, from `base` down to the tile's bottom (the design's
    chained quadratic curves, each 206 wide, rising `amp` above and dipping below the base)."""
    pts = []
    x = x0
    while x <= x1:
        pts.append((scale(x), scale(base - amp * math.sin(2 * math.pi * (x - x0) / period))))
        x += 2
    pts += [(scale(x1), scale(x1)), (scale(x0), scale(x1))]
    d.polygon(pts, fill=fill)


def dashes(d, x0, x1, y, width, dash, gap, fill, scale=px):
    """A dashed line with round caps, centred between x0 and x1 (SVG dasharray + round linecap)."""
    n = max(1, int((x1 - x0 + gap) // (dash + gap)))
    span = n * dash + (n - 1) * gap
    x = x0 + (x1 - x0 - span) / 2
    for _ in range(n):
        d.rounded_rectangle(
            [
                scale(x - width / 2),
                scale(y - width / 2),
                scale(x + dash + width / 2),
                scale(y + width / 2),
            ],
            radius=scale(width / 2),
            fill=fill,
        )
        x += dash + gap


def app_icon():
    art = Image.new("RGBA", (px(S), px(S)), (*DEEP, 255))
    d = ImageDraw.Draw(art)
    # The older, fainter mark: a thin light line at 35%.
    faint = Image.new("RGBA", art.size, (0, 0, 0, 0))
    ImageDraw.Draw(faint).rectangle(
        [px(100), px(472 - 7), px(924), px(472 + 7)], fill=(*OLD_MARK, 89)
    )
    art = Image.alpha_composite(art, faint)
    d = ImageDraw.Draw(art)
    wave(d, 600, 27.5, WAVE)
    wave(d, 710, 22.5, WAVE_LOW)
    # The design's 64/52 dasharray leaves 6 units between round caps 46 wide;
    # wider gaps read as dashes.
    dashes(d, 230, 794, 370, 46, 34, 70, MARK)
    # Clip to the tile: transparent corners.
    mask = Image.new("L", art.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [px(100), px(100), px(924), px(924)], radius=px(186), fill=255
    )
    img = Image.new("RGBA", art.size, (0, 0, 0, 0))
    img.paste(art, (0, 0), mask)
    out = ROOT / "design/icon.png"
    img.resize((S, S), Image.LANCZOS).save(out)
    print(f"wrote {out}")


def tray_icon(alert):
    """The menu-bar glyph, black on alpha, 36px (18pt @2x), from the design's 18-unit drawing: a
    rounded square outline, a dashed line in its upper part and a wave below. The alert version
    cuts a notch in the top-right corner and puts a filled dot in it."""
    T = 36
    ss = 16
    u = T * ss / 18  # one design unit
    img = Image.new("L", (T * ss, T * ss), 0)
    d = ImageDraw.Draw(img)
    w = 1.5 * u
    d.rounded_rectangle(
        [1.75 * u - w / 2, 1.75 * u - w / 2, 16.25 * u + w / 2, 16.25 * u + w / 2],
        radius=4.75 * u,
        fill=255,
    )
    d.rounded_rectangle(
        [1.75 * u + w / 2, 1.75 * u + w / 2, 16.25 * u - w / 2, 16.25 * u - w / 2],
        radius=3.25 * u,
        fill=0,
    )
    # The dashed tidemark: 2-unit dashes, 1.6 gaps, round caps, from x=5 to 13 at y=6.5.
    x = 5.0
    while x < 13:
        e = min(x + 2, 13)
        d.rounded_rectangle(
            [x * u - w / 2, 6.5 * u - w / 2, e * u + w / 2, 6.5 * u + w / 2], radius=w / 2, fill=255
        )
        x += 3.6
    # The wave: three crests between x=3.2 and 14.9 around y=11.6, stroked.
    pts = []
    for i in range(1171):
        xx = 3.2 + i / 100
        # Crest then trough, each 3.9 units wide (the design's chained curves).
        yy = 11.6 - 0.95 * math.sin(math.pi * (xx - 3.2) / 3.9)
        pts.append((xx * u, yy * u))
    for a, b in itertools.pairwise(pts):
        d.line([a, b], fill=255, width=round(w))
    for p in (pts[0], pts[-1]):
        d.ellipse([p[0] - w / 2, p[1] - w / 2, p[0] + w / 2, p[1] + w / 2], fill=255)
    if alert:
        cx, cy, r = 15.2 * u, 2.8 * u, 2.6 * u
        d.ellipse([cx - r - 1.1 * u, cy - r - 1.1 * u, cx + r + 1.1 * u, cy + r + 1.1 * u], fill=0)
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=255)
    alpha = img.resize((T, T), Image.LANCZOS)
    out = ROOT / f"src-tauri/icons/tray{'-alert' if alert else ''}@2x.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    glyph = Image.new("RGBA", (T, T), (0, 0, 0, 0))
    glyph.putalpha(alpha)
    glyph.save(out)
    print(f"wrote {out}")


if __name__ == "__main__":
    app_icon()
    tray_icon(alert=False)
    tray_icon(alert=True)
