#!/usr/bin/env python3
"""Optional SVG-to-PNG capture conversion, using installed librsvg and libcairo.

This is a documentation helper, never loaded by the application.
"""
import ctypes
import ctypes.util
from pathlib import Path
import sys


def convert(path):
    rsvg = ctypes.CDLL(ctypes.util.find_library("rsvg-2"))
    cairo = ctypes.CDLL(ctypes.util.find_library("cairo"))
    gobject = ctypes.CDLL(ctypes.util.find_library("gobject-2.0"))
    rsvg.rsvg_handle_new_from_file.argtypes = [ctypes.c_char_p, ctypes.c_void_p]
    rsvg.rsvg_handle_new_from_file.restype = ctypes.c_void_p
    rsvg.rsvg_handle_render_cairo.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    rsvg.rsvg_handle_render_cairo.restype = ctypes.c_int
    cairo.cairo_image_surface_create.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
    cairo.cairo_image_surface_create.restype = ctypes.c_void_p
    cairo.cairo_create.argtypes = [ctypes.c_void_p]
    cairo.cairo_create.restype = ctypes.c_void_p
    cairo.cairo_surface_write_to_png.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    cairo.cairo_surface_write_to_png.restype = ctypes.c_int
    cairo.cairo_destroy.argtypes = [ctypes.c_void_p]
    cairo.cairo_surface_destroy.argtypes = [ctypes.c_void_p]
    gobject.g_object_unref.argtypes = [ctypes.c_void_p]
    handle = rsvg.rsvg_handle_new_from_file(str(path).encode(), None)
    if not handle:
        raise RuntimeError(f"cannot load {path}")
    # Capture canvases embed their pixel dimensions; read those exact dimensions.
    import xml.etree.ElementTree as ET
    root = ET.parse(path).getroot()
    width, height = int(root.attrib["width"]), int(root.attrib["height"])
    surface = cairo.cairo_image_surface_create(0, width, height)
    context = cairo.cairo_create(surface)
    try:
        if not rsvg.rsvg_handle_render_cairo(handle, context):
            raise RuntimeError("SVG rendering failed")
        if cairo.cairo_surface_write_to_png(surface, str(path.with_suffix(".png")).encode()):
            raise RuntimeError("PNG write failed")
    finally:
        cairo.cairo_destroy(context)
        cairo.cairo_surface_destroy(surface)
        gobject.g_object_unref(handle)


for name in sys.argv[1:] or ("captures/primary.svg", "captures/secondary.svg"):
    convert(Path(name))
