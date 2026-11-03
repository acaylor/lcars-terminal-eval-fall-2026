#!/usr/bin/env python3
"""Rasterize our cell SVGs for review. Optional development dependency: Pillow."""
import argparse
from pathlib import Path
import xml.etree.ElementTree as ET
from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--font", default="/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
parser.add_argument("paths", nargs="+", type=Path)
args = parser.parse_args()
scale = 2
font = ImageFont.truetype(args.font, 16 * scale)
for path in args.paths:
    root = ET.parse(path).getroot()
    width, height = int(root.attrib["width"]), int(root.attrib["height"])
    image = Image.new("RGB", (width * scale, height * scale), "#090c12")
    draw = ImageDraw.Draw(image)
    for element in root.iter():
        tag = element.tag.split("}")[-1]
        attrs = element.attrib
        if tag == "rect" and "%" not in attrs.get("width", ""):
            x, y = int(attrs.get("x", 0)), int(attrs.get("y", 0))
            w, h = int(attrs["width"]), int(attrs["height"])
            draw.rectangle((x*scale, y*scale, (x+w)*scale-1, (y+h)*scale-1), fill=attrs["fill"])
        elif tag == "text":
            draw.text((int(attrs["x"])*scale, int(attrs["y"])*scale), element.text or "", font=font, fill=attrs["fill"], anchor="ls")
        elif tag == "circle":
            x, y, r = int(attrs["cx"]), int(attrs["cy"]), int(attrs["r"])
            draw.ellipse(((x-r)*scale, (y-r)*scale, (x+r)*scale, (y+r)*scale), fill=attrs["fill"])
    image.resize((width, height), Image.Resampling.LANCZOS).save(path.with_suffix(".png"))
    print(path.with_suffix(".png"))
