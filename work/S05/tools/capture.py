#!/usr/bin/env python3
"""Capture harness: run lcars in a pty at a fixed size, send keys, dump the
raw byte stream to a file for offline rendering.

Usage: capture.py OUT PRIMARY|DIAG [W H]
"""
import fcntl, os, re, select, struct, subprocess, sys, termios, time

out = sys.argv[1]
mode = sys.argv[2] if len(sys.argv) > 2 else "PRIMARY"
W = int(sys.argv[3]) if len(sys.argv) > 3 else 120
H = int(sys.argv[4]) if len(sys.argv) > 4 else 36

import pty

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.environ["COLORTERM"] = "truecolor"
    os.environ["LANG"] = "C.UTF-8"
    try:
        os.execvp("./bin/lcars", ["./bin/lcars"])
    except Exception:
        os._exit(127)

fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", H, W, 0, 0))

def drain(dur):
    data = b""
    end = time.time() + dur
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.1)
        if r:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            data += chunk
    return data

raw = drain(4.2)
if mode == "DIAG":
    os.write(fd, b"p")
    raw += drain(1.6)

ANSI_RE = re.compile(rb"\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07]*\x07|\x1b[()#][0-9A-Za-z]")

def render(data):
    grid = [[" "] * W for _ in range(H)]
    x = y = 0
    i = 0
    while i < len(data):
        b0 = data[i]
        m = ANSI_RE.match(data, i)
        if m and b0 == 0x1B:
            seq = m.group(0)
            if seq.startswith(b"\x1b[") and seq.endswith(b"H"):
                parts = seq[2:-1].split(b";")
                try:
                    y = max(0, int(parts[0] or b"1") - 1)
                    x = max(0, int(parts[1] or b"1") - 1)
                except Exception:
                    pass
            elif seq == b"\x1b[H":
                x = y = 0
            elif seq.endswith(b"J"):
                grid = [[" "] * W for _ in range(H)]
                x = y = 0
            i = m.end()
            continue
        if b0 == 0x0D:
            x = 0; i += 1; continue
        if b0 == 0x0A:
            y = min(H - 1, y + 1); x = 0; i += 1; continue
        if b0 < 0x20:
            i += 1; continue
        # decode one rune
        try:
            ch, n = data[i:].decode("utf-8")[0], len(data[i:].decode("utf-8")[0].encode("utf-8"))
            txt = data[i:].decode("utf-8")[:1]
            n = len(txt.encode("utf-8"))
        except Exception:
            i += 1; continue
        # wide chars occupy 2 cols
        import unicodedata
        wch = 2 if unicodedata.east_asian_width(txt) in ("W", "F") else 1
        for k in range(wch):
            if 0 <= x + k < W and 0 <= y < H:
                grid[y][x + k] = txt if k == 0 else " "
        x += wch
        if x >= W:
            x = 0; y = min(H - 1, y + 1)
        i += n
    return "\n".join("".join(r) for r in grid)

grid = render(raw)
open(out, "w", encoding="utf-8").write(grid + "\n")

# save the rune grid
nonce = os.path.basename(out)
# exit
try:
    os.write(fd, b"q")
    time.sleep(0.3)
    _, status = os.waitpid(pid, os.WNOHANG)
    if status == 0:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
except Exception:
    pass
print("wrote", out, "bytes raw:", len(raw))
PYEOF=1
print(open(out, encoding="utf-8").read())
