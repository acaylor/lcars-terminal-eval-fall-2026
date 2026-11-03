#!/usr/bin/env python3
"""Convert a `tmux capture-pane -e -p` dump (SGR colour escapes) into an SVG.

Used only to turn the terminal captures in captures/ into images that can be
viewed without a terminal. Handles 24-bit, 256-colour and basic SGR codes.
Usage: ansi2svg.py INPUT.ansi OUTPUT.svg [COLS ROWS]
"""
import html
import re
import sys

CELL_W, CELL_H = 9.0, 19.0
BASIC = ["#000000", "#cc0000", "#4e9a06", "#c4a000", "#3465a4", "#75507b", "#06989a", "#d3d7cf",
         "#555753", "#ef2929", "#8ae234", "#fce94f", "#729fcf", "#ad7fa8", "#34e2e2", "#eeeeec"]


def ansi256(n):
    if n < 16:
        return BASIC[n]
    if n >= 232:
        v = 8 + (n - 232) * 10
        return "#%02x%02x%02x" % (v, v, v)
    n -= 16
    r, g, b = n // 36, (n // 6) % 6, n % 6
    conv = lambda c: 0 if c == 0 else 55 + c * 40
    return "#%02x%02x%02x" % (conv(r), conv(g), conv(b))


def parse(text):
    """Yield rows of (char, fg, bg, bold)."""
    fg, bg, bold, rev = None, None, False, False
    rows = []
    for line in text.split("\n"):
        row = []
        i = 0
        while i < len(line):
            m = re.match(r"\x1b\[([0-9;]*)m", line[i:])
            if m:
                params = [int(p) if p else 0 for p in m.group(1).split(";")] or [0]
                j = 0
                while j < len(params):
                    p = params[j]
                    if p == 0:
                        fg, bg, bold, rev = None, None, False, False
                    elif p == 1:
                        bold = True
                    elif p == 7:
                        rev = True
                    elif p == 39:
                        fg = None
                    elif p == 49:
                        bg = None
                    elif 30 <= p <= 37:
                        fg = BASIC[p - 30]
                    elif 90 <= p <= 97:
                        fg = BASIC[p - 90 + 8]
                    elif 40 <= p <= 47:
                        bg = BASIC[p - 40]
                    elif p in (38, 48) and j + 1 < len(params):
                        if params[j + 1] == 2 and j + 4 < len(params):
                            col = "#%02x%02x%02x" % tuple(params[j + 2:j + 5])
                            j += 4
                        elif params[j + 1] == 5 and j + 2 < len(params):
                            col = ansi256(params[j + 2])
                            j += 2
                        else:
                            col = None
                        if p == 38:
                            fg = col
                        else:
                            bg = col
                    j += 1
                i += m.end()
                continue
            if line[i] == "\x1b":
                i += 1
                continue
            f, b = (bg or "#000000", fg or "#f0f0f0") if rev else (fg, bg)
            row.append((line[i], f, b, bold))
            i += 1
        rows.append(row)
    return rows


def main():
    src, dst = sys.argv[1], sys.argv[2]
    cols = int(sys.argv[3]) if len(sys.argv) > 3 else 120
    rows_n = int(sys.argv[4]) if len(sys.argv) > 4 else 36
    rows = parse(open(src, encoding="utf-8").read())[:rows_n]
    w, h = cols * CELL_W, rows_n * CELL_H
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0f}" height="{h:.0f}" viewBox="0 0 {w:.0f} {h:.0f}">',
           f'<rect width="{w:.0f}" height="{h:.0f}" fill="#000"/>',
           '<style>text{font-family:"DejaVu Sans Mono","Menlo","Consolas",monospace;font-size:15px;'
           'dominant-baseline:central;white-space:pre}</style>']
    for y, row in enumerate(rows):
        x = 0
        for ch, fg, bg, bold in row:
            if bg:
                out.append(f'<rect x="{x * CELL_W:.1f}" y="{y * CELL_H:.1f}" width="{CELL_W + 0.3:.1f}" height="{CELL_H + 0.3:.1f}" fill="{bg}"/>')
            if ch not in (" ",):
                weight = ' font-weight="bold"' if bold else ""
                color = fg or "#e8e8e8"
                # Block elements are drawn as rectangles so the silhouette is
                # exact regardless of the viewer's font.
                rect = block_rect(ch)
                if rect:
                    rx, ry, rw, rh = rect
                    out.append(f'<rect x="{x * CELL_W + rx * CELL_W:.1f}" y="{y * CELL_H + ry * CELL_H:.1f}" '
                               f'width="{rw * CELL_W + 0.3:.1f}" height="{rh * CELL_H + 0.3:.1f}" fill="{color}"/>')
                else:
                    out.append(f'<text x="{x * CELL_W:.1f}" y="{y * CELL_H + CELL_H / 2:.1f}" fill="{color}"{weight}>{html.escape(ch)}</text>')
            x += 1
    out.append("</svg>")
    open(dst, "w", encoding="utf-8").write("\n".join(out))


def block_rect(ch):
    table = {
        "█": (0, 0, 1, 1), "▀": (0, 0, 1, 0.5), "▄": (0, 0.5, 1, 0.5), "▌": (0, 0, 0.5, 1), "▐": (0.5, 0, 0.5, 1),
        "▗": (0.5, 0.5, 0.5, 0.5), "▖": (0, 0.5, 0.5, 0.5), "▝": (0.5, 0, 0.5, 0.5), "▘": (0, 0, 0.5, 0.5),
        "▁": (0, 0.875, 1, 0.125), "▂": (0, 0.75, 1, 0.25), "▃": (0, 0.625, 1, 0.375), "▅": (0, 0.375, 1, 0.625),
        "▆": (0, 0.25, 1, 0.75), "▇": (0, 0.125, 1, 0.875),
        "▏": (0, 0, 0.125, 1), "▎": (0, 0, 0.25, 1), "▍": (0, 0, 0.375, 1), "▋": (0, 0, 0.625, 1),
        "▊": (0, 0, 0.75, 1), "▉": (0, 0, 0.875, 1),
    }
    return table.get(ch)


if __name__ == "__main__":
    main()
