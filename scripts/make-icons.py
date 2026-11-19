#!/usr/bin/env python3
"""Draws the anyzoom icon set into app/src-tauri/icons from the 3E mark.

The icon is the 3E mark inside a camera crop frame. Small sizes (tray,
taskbar) drop the frame and thicken the mark so it stays readable.
Needs: pip install cairosvg pillow.   Run: python3 scripts/make-icons.py
"""
import io
import pathlib
import re

import cairosvg
from PIL import Image

ICONS = pathlib.Path(__file__).resolve().parent.parent / "app/src-tauri/icons"
MARK = re.search(r'<path d="([^"]+)"', (ICONS / "3e-mark.svg").read_text()).group(1)
INK, PAPER, COBALT = "#0B0B0C", "#FAFAF8", "#1F3CFF"
# the mark's own drawing box: x, y, width, height
MX, MY, MW, MH = 329, 529, 1321, 1022


def mark(width, thicken=0.0):
    scale = width / MW
    x, y = (512 - width) / 2, (512 - MH * scale) / 2
    stroke = f' stroke="{PAPER}" stroke-width="{thicken / scale}" stroke-linejoin="round"' if thicken else ""
    return f'<g transform="translate({x} {y}) scale({scale}) translate({-MX} {-MY})"><path d="{MARK}" fill="{PAPER}"{stroke}/></g>'


def frame(inset=62, arm=92, width=24):
    a, b = inset, 512 - inset
    corners = [
        f"M{a} {a + arm}V{a}H{a + arm}", f"M{b - arm} {a}H{b}V{a + arm}",
        f"M{b} {b - arm}V{b}H{b - arm}", f"M{a + arm} {b}H{a}V{b - arm}",
    ]
    return f'<path d="{" ".join(corners)}" fill="none" stroke="{COBALT}" stroke-width="{width}"/>'


def svg(size):
    if size >= 64:
        body = frame() + mark(310, thicken=7)
    else:
        # no frame; the smaller the picture, the heavier the lines
        body = mark(410, thicken=26 if size <= 24 else 16)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">'
            f'<rect width="512" height="512" rx="112" fill="{INK}"/>{body}</svg>')


def png(size):
    data = cairosvg.svg2png(bytestring=svg(size).encode(), output_width=size, output_height=size)
    return Image.open(io.BytesIO(data)).convert("RGBA")


(ICONS / "icon.svg").write_text(svg(512))
png(32).save(ICONS / "32x32.png")
png(512).save(ICONS / "icon.png")
sizes = [256, 128, 64, 48, 32, 24, 16]
images = [png(s) for s in sizes]
images[0].save(ICONS / "icon.ico", sizes=[(s, s) for s in sizes], append_images=images[1:])
print("wrote", ", ".join(p.name for p in sorted(ICONS.iterdir())))
