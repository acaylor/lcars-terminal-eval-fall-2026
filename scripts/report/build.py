#!/usr/bin/env python3
"""Build the HTML report from template.html and the capture SVGs in captures-fresh/.

Usage: python3 scripts/report/build.py [--fragment out.html]
Writes docs/index.html (a standalone page, served by GitHub Pages). --fragment also writes
the bare page body used for a claude.ai Artifact.
"""
import argparse, base64, json, pathlib

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent

ap = argparse.ArgumentParser()
ap.add_argument("--fragment", type=pathlib.Path)
args = ap.parse_args()

shots = {}
for i in range(1, 10):
    sid = f"S{i:02d}"
    shots[sid] = {
        key: "data:image/svg+xml;base64," + base64.b64encode((ROOT / "captures-fresh" / sid / name).read_bytes()).decode()
        for key, name in (("p", "primary-120x36.svg"), ("d", "diagnostic-120x36.svg"))
    }

page = (HERE / "template.html").read_text().replace("/*__SHOTS__*/{}", json.dumps(shots, separators=(",", ":")))
if args.fragment:
    args.fragment.write_text(page)
head, body = page.split("<!--BODY-->", 1)
out = ROOT / "docs" / "index.html"
out.write_text(
    "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n"
    "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n"
    + head + "</head>\n<body>\n" + body + "\n</body>\n</html>\n"
)
print(out, out.stat().st_size)
