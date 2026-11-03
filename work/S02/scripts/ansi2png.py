#!/usr/bin/env python3
"""Render an `lcarsmon --snapshot ... --ansi` capture to PNG.

Development aid for reviewing compositions and producing README images.
Block, quadrant and Braille glyphs are drawn geometrically (as a terminal
would), everything else with DejaVu Sans Mono. Requires Pillow:

    uv run --with pillow scripts/ansi2png.py capture.ansi out.png
"""
import re
import sys
from PIL import Image, ImageDraw, ImageFont

CW, CH = 11, 22
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_B = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
QUAD = {' ': 0, '▘': 1, '▝': 2, '▀': 3, '▖': 4, '▌': 5, '▞': 6, '▛': 7, '▗': 8,
        '▚': 9, '▐': 10, '▜': 11, '▄': 12, '▙': 13, '▟': 14, '█': 15}
EIGHTHS = {'▁': 1, '▂': 2, '▃': 3, '▄': 4, '▅': 5, '▆': 6, '▇': 7}


def parse(text):
    rows = []
    for line in text.split("\n"):
        if not line:
            continue
        fg, bg, bold, rev = (230, 230, 230), (0, 0, 0), False, False
        cells = []
        for part in re.split(r"(\x1b\[[0-9;]*m)", line):
            if part.startswith("\x1b["):
                codes = [int(c) if c else 0 for c in part[2:-1].split(";")]
                i = 0
                while i < len(codes):
                    c = codes[i]
                    if c == 0:
                        fg, bg, bold, rev = (230, 230, 230), (0, 0, 0), False, False
                    elif c == 1:
                        bold = True
                    elif c == 7:
                        rev = True
                    elif c in (38, 48) and codes[i + 1] == 2:
                        col = tuple(codes[i + 2:i + 5])
                        if c == 38:
                            fg = col
                        else:
                            bg = col
                        i += 4
                    i += 1
            else:
                for ch in part:
                    f, b = (bg, fg) if rev else (fg, bg)
                    cells.append((ch, f, b, bold))
        rows.append(cells)
    return rows


def main(src, dst):
    rows = parse(open(src, encoding="utf-8").read())
    w = max(len(r) for r in rows)
    img = Image.new("RGB", (w * CW, len(rows) * CH), (0, 0, 0))
    d = ImageDraw.Draw(img)
    font = ImageFont.truetype(FONT, 17)
    font_b = ImageFont.truetype(FONT_B, 17)
    for y, row in enumerate(rows):
        for x, (ch, fg, bg, bold) in enumerate(row):
            x0, y0 = x * CW, y * CH
            d.rectangle([x0, y0, x0 + CW - 1, y0 + CH - 1], fill=bg)
            if ch in QUAD:
                m = QUAD[ch]
                hw, hh = CW // 2, CH // 2
                for bit, (qx, qy) in enumerate([(0, 0), (1, 0), (0, 1), (1, 1)]):
                    if m & (1 << bit):
                        ax = x0 + qx * hw
                        ay = y0 + qy * hh
                        d.rectangle([ax, ay, (x0 + CW - 1) if qx else (ax + hw - 1), (y0 + CH - 1) if qy else (ay + hh - 1)], fill=fg)
            elif ch in EIGHTHS:
                hgt = CH * EIGHTHS[ch] // 8
                d.rectangle([x0, y0 + CH - hgt, x0 + CW - 1, y0 + CH - 1], fill=fg)
            elif 0x2800 <= ord(ch) <= 0x28FF:
                bits = ord(ch) - 0x2800
                order = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (0, 3), (1, 3)]
                for bit, (dx, dy) in enumerate(order):
                    if bits & (1 << bit):
                        cx = x0 + 3 + dx * 5
                        cy = y0 + 3 + dy * 5
                        d.ellipse([cx - 1.5, cy - 1.5, cx + 1.5, cy + 1.5], fill=fg)
            elif ch in "◖◗":
                if ch == "◖":
                    d.pieslice([x0, y0 + 1, x0 + 2 * CW - 1, y0 + CH - 2], 90, 270, fill=fg)
                else:
                    d.pieslice([x0 - CW, y0 + 1, x0 + CW - 1, y0 + CH - 2], 270, 90, fill=fg)
            elif ch != " ":
                d.text((x0, y0 + 1), ch, font=font_b if bold else font, fill=fg)
    img.save(dst)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
