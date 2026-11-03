"""Reusable PTY + virtual-terminal harness for the LCARS evaluation.

Spawns a binary on a fresh pseudo-terminal (as session leader with the PTY as its
controlling terminal), mirrors its output into a pyte virtual screen, answers common
terminal queries (cursor position, primary device attributes), and offers helpers to
capture screens, analyse escape sequences, and render captures to SVG/PNG.

Works on Linux and macOS (pure stdlib + pyte; cairosvg optional for PNG).
"""
from __future__ import annotations

import fcntl
import html
import json
import os
import platform
import re
import select
import signal
import socket
import struct
import subprocess
import termios
import threading
import time
from dataclasses import dataclass, field

import pyte

# ---------------------------------------------------------------- PTY session

QUERY_CPR = b"\x1b[6n"
QUERY_DA1 = re.compile(rb"\x1b\[0?c")
QUERY_DA2 = re.compile(rb"\x1b\[>0?c")
QUERY_OSC_COLOR = re.compile(rb"\x1b\](1[01]);\?(\x07|\x1b\\)")
QUERY_KITTY_KBD = b"\x1b[?u"


def set_winsize(fd: int, rows: int, cols: int) -> None:
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


@dataclass
class Chunk:
    t: float
    data: bytes


class PtySession:
    """One child process attached to its own PTY, with a background reader."""

    def __init__(self, argv, cols=120, rows=36, env_extra=None, env_unset=(), cwd=None):
        self.cols, self.rows = cols, rows
        self.master, self.slave = os.openpty()
        set_winsize(self.slave, rows, cols)
        self.termios_before = termios.tcgetattr(self.slave)
        env = dict(os.environ)
        for k in ("NO_COLOR", "COLORTERM", "LCARS_MONO", "LCARS_ASCII", "LC_ALL", "LANG", "TMUX", "STY"):
            env.pop(k, None)
        env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor", "LANG": "en_US.UTF-8",
                    "COLUMNS": str(cols), "LINES": str(rows)})
        for k in env_unset:
            env.pop(k, None)
        if env_extra:
            env.update(env_extra)
        self.env = env
        self.chunks: list[Chunk] = []
        self.lock = threading.Lock()
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.ByteStream(self.screen)
        self.queries_answered: dict[str, int] = {}
        self._qbuf = b""
        slave = self.slave

        def preexec():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        self.t_spawn = time.monotonic()
        self.proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, env=env,
                                     cwd=cwd, preexec_fn=preexec, close_fds=True)
        self.t_first_byte: float | None = None
        self._stop = False
        self.reader = threading.Thread(target=self._read_loop, daemon=True)
        self.reader.start()

    # -- reader ---------------------------------------------------------------
    def _answer_queries(self, data: bytes) -> None:
        buf = self._qbuf + data
        replies = []
        n = buf.count(QUERY_CPR)
        for _ in range(n):
            y, x = self.screen.cursor.y + 1, self.screen.cursor.x + 1
            replies.append(f"\x1b[{y};{x}R".encode())
            self.queries_answered["CPR"] = self.queries_answered.get("CPR", 0) + 1
        for _ in QUERY_DA2.finditer(buf):
            replies.append(b"\x1b[>41;380;0c")
            self.queries_answered["DA2"] = self.queries_answered.get("DA2", 0) + 1
        for _ in QUERY_DA1.finditer(buf):
            replies.append(b"\x1b[?62;22c")
            self.queries_answered["DA1"] = self.queries_answered.get("DA1", 0) + 1
        for m in QUERY_OSC_COLOR.finditer(buf):
            which = m.group(1).decode()
            col = "ffff/ffff/ffff" if which == "10" else "0000/0000/0000"
            replies.append(f"\x1b]{which};rgb:{col}\x1b\\".encode())
            self.queries_answered["OSC" + which] = self.queries_answered.get("OSC" + which, 0) + 1
        # carry over only an *incomplete* trailing escape sequence so a query split
        # across reads is answered exactly once
        idx = buf.rfind(b"\x1b")
        tail = buf[idx:] if idx != -1 and len(buf) - idx < 8 else b""
        if len(tail) > 2 and re.search(rb"[A-Za-z~\x07]", tail[2:]):
            tail = b""
        self._qbuf = tail
        for r in replies:
            try:
                os.write(self.master, r)
            except OSError:
                pass

    def _read_loop(self):
        while not self._stop:
            try:
                r, _, _ = select.select([self.master], [], [], 0.05)
            except (OSError, ValueError):
                break
            if not r:
                if self.proc.poll() is not None and not self._pending():
                    # drain finished
                    time.sleep(0.05)
                    if not self._pending():
                        break
                continue
            try:
                data = os.read(self.master, 65536)
            except OSError:
                break
            if not data:
                break
            now = time.monotonic()
            with self.lock:
                if self.t_first_byte is None:
                    self.t_first_byte = now
                self.chunks.append(Chunk(now, data))
                try:
                    self.stream.feed(data)
                except Exception:  # pyte robustness: never let a parse error kill the reader
                    pass
            self._answer_queries(data)

    def _pending(self) -> bool:
        try:
            r, _, _ = select.select([self.master], [], [], 0)
            return bool(r)
        except (OSError, ValueError):
            return False

    # -- interaction ----------------------------------------------------------
    def send(self, data: bytes | str) -> float:
        if isinstance(data, str):
            data = data.encode()
        try:
            os.write(self.master, data)
        except OSError:  # child already gone (macOS returns EIO on the master)
            pass
        return time.monotonic()

    def resize(self, cols: int, rows: int, explicit_signal=True) -> None:
        with self.lock:
            self.cols, self.rows = cols, rows
            self.screen.resize(rows, cols)
        set_winsize(self.slave, rows, cols)  # kernel delivers SIGWINCH to fg pgrp
        if explicit_signal and self.alive():
            try:
                os.kill(self.proc.pid, signal.SIGWINCH)
            except ProcessLookupError:
                pass

    def alive(self) -> bool:
        return self.proc.poll() is None

    def signal(self, sig) -> float:
        t = time.monotonic()
        try:
            os.kill(self.proc.pid, sig)
        except ProcessLookupError:
            pass
        return t

    def wait_exit(self, timeout: float) -> int | None:
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            return None

    def total_bytes(self) -> int:
        with self.lock:
            return sum(len(c.data) for c in self.chunks)

    def bytes_since(self, t0: float, t1: float | None = None) -> bytes:
        with self.lock:
            return b"".join(c.data for c in self.chunks if c.t >= t0 and (t1 is None or c.t <= t1))

    def all_bytes(self) -> bytes:
        with self.lock:
            return b"".join(c.data for c in self.chunks)

    def wait_quiet(self, settle=0.25, max_wait=2.0):
        """Wait until no output for `settle` seconds (or max_wait)."""
        end = time.monotonic() + max_wait
        while time.monotonic() < end:
            with self.lock:
                last = self.chunks[-1].t if self.chunks else self.t_spawn
            if time.monotonic() - last >= settle:
                return
            time.sleep(0.05)

    def snapshot(self) -> "ScreenSnap":
        with self.lock:
            return ScreenSnap.from_screen(self.screen)

    def close(self, kill=True):
        if kill and self.alive():
            try:
                os.kill(self.proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.wait_exit(2)
        time.sleep(0.1)
        self._stop = True
        self.reader.join(timeout=1)
        for fd in (self.master, self.slave):
            try:
                os.close(fd)
            except OSError:
                pass

    def termios_now(self):
        return termios.tcgetattr(self.slave)


# ---------------------------------------------------------------- screen snapshots

BASIC = {
    "black": "000000", "red": "cd3131", "green": "0dbc79", "brown": "e5e510", "yellow": "e5e510",
    "blue": "2472c8", "magenta": "bc3fbc", "cyan": "11a8cd", "white": "e5e5e5",
    "brightblack": "666666", "brightred": "f14c4c", "brightgreen": "23d18b", "brightbrown": "f5f543",
    "brightyellow": "f5f543", "brightblue": "3b8eea", "brightmagenta": "d670d6", "brightcyan": "29b8db",
    "brightwhite": "ffffff",
}
DEFAULT_FG, DEFAULT_BG = "d0d0d0", "000000"


def color_hex(c: str, default: str) -> str:
    if not c or c == "default":
        return default
    if re.fullmatch(r"[0-9a-fA-F]{6}", c):
        return c.lower()
    return BASIC.get(c.replace("_", "").lower(), default)


@dataclass
class ScreenSnap:
    cols: int
    rows: int
    cells: list  # rows x cols of (char, fg, bg, bold, reverse)

    @classmethod
    def from_screen(cls, screen: pyte.Screen) -> "ScreenSnap":
        cells = []
        for y in range(screen.lines):
            line = screen.buffer[y]
            row = []
            for x in range(screen.columns):
                ch = line[x]
                row.append((ch.data, ch.fg, ch.bg, ch.bold, ch.reverse))
            cells.append(row)
        return cls(screen.columns, screen.lines, cells)

    def text(self) -> str:
        return "\n".join("".join(c[0] for c in row).rstrip() for row in self.cells)

    def text_nodigits(self) -> str:
        return re.sub(r"\d", "#", self.text())

    def non_ascii_chars(self) -> dict:
        out: dict = {}
        for row in self.cells:
            for c in row:
                for ch in c[0]:
                    if ord(ch) > 126:
                        out[ch] = out.get(ch, 0) + 1
        return out

    def blank_fraction(self) -> float:
        """Fraction of cells that are a space with the default/black background."""
        blank = 0
        for row in self.cells:
            for ch, fg, bg, bold, rev in row:
                if ch in ("", " ") and not rev and color_hex(bg, DEFAULT_BG) == DEFAULT_BG:
                    blank += 1
        return blank / (self.cols * self.rows)

    def distinct_colors(self) -> int:
        s = set()
        for row in self.cells:
            for ch, fg, bg, bold, rev in row:
                s.add(color_hex(fg, DEFAULT_FG))
                s.add(color_hex(bg, DEFAULT_BG))
        return len(s)

    def to_ansi(self) -> str:
        """Replayable ANSI rendering of the screen state (truecolor SGR)."""
        out = []
        for row in self.cells:
            prev = None
            line = []
            for ch, fg, bg, bold, rev in row:
                f, b = color_hex(fg, DEFAULT_FG), color_hex(bg, DEFAULT_BG)
                if rev:
                    f, b = b, f
                key = (f, b, bold)
                if key != prev:
                    line.append("\x1b[0;%s38;2;%d;%d;%d;48;2;%d;%d;%dm" % (
                        "1;" if bold else "", int(f[0:2], 16), int(f[2:4], 16), int(f[4:6], 16),
                        int(b[0:2], 16), int(b[2:4], 16), int(b[4:6], 16)))
                    prev = key
                line.append(ch if ch else "")
            out.append("".join(line) + "\x1b[0m")
        return "\n".join(out) + "\n"

    def to_svg(self, title="") -> str:
        cw, ch_h = 9.0, 18.0
        w, h = self.cols * cw, self.rows * ch_h
        parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0f}" height="{h:.0f}" '
                 f'viewBox="0 0 {w:.0f} {h:.0f}" font-family="DejaVu Sans Mono, Menlo, monospace" '
                 f'font-size="15">', f"<title>{html.escape(title)}</title>",
                 f'<rect width="100%" height="100%" fill="#{DEFAULT_BG}"/>']
        for y, row in enumerate(self.cells):
            # background runs
            x = 0
            while x < self.cols:
                ch, fg, bg, bold, rev = row[x]
                b = color_hex(fg, DEFAULT_FG) if rev else color_hex(bg, DEFAULT_BG)
                x2 = x + 1
                while x2 < self.cols:
                    c2 = row[x2]
                    b2 = color_hex(c2[1], DEFAULT_FG) if c2[4] else color_hex(c2[2], DEFAULT_BG)
                    if b2 != b:
                        break
                    x2 += 1
                if b != DEFAULT_BG:
                    parts.append(f'<rect x="{x*cw:.1f}" y="{y*ch_h:.1f}" width="{(x2-x)*cw+0.6:.1f}" '
                                 f'height="{ch_h+0.6:.1f}" fill="#{b}"/>')
                x = x2
            for x, (ch, fg, bg, bold, rev) in enumerate(row):
                if not ch or ch == " ":
                    continue
                f = color_hex(bg, DEFAULT_BG) if rev else color_hex(fg, DEFAULT_FG)
                wt = ' font-weight="bold"' if bold else ""
                parts.append(f'<text x="{x*cw:.1f}" y="{y*ch_h+14:.1f}" fill="#{f}"{wt}>'
                             f'{html.escape(ch)}</text>')
        parts.append("</svg>")
        return "\n".join(parts)


def save_capture(snap: ScreenSnap, base: str, png=True, raw: bytes | None = None) -> None:
    with open(base + ".txt", "w") as fh:
        fh.write(snap.text() + "\n")
    with open(base + ".ansi", "w") as fh:
        fh.write(snap.to_ansi())
    svg = snap.to_svg(os.path.basename(base))
    with open(base + ".svg", "w") as fh:
        fh.write(svg)
    if raw is not None:
        with open(base + ".raw", "wb") as fh:
            fh.write(raw)
    if png:
        try:
            import cairosvg
            cairosvg.svg2png(bytestring=svg.encode(), write_to=base + ".png")
        except Exception:
            pass


# ---------------------------------------------------------------- escape analysis

SGR_RE = re.compile(rb"\x1b\[([0-9;:]*)m")


def sgr_stats(data: bytes) -> dict:
    tc = c256 = basic = other = 0
    for m in SGR_RE.finditer(data):
        params = m.group(1).replace(b":", b";").split(b";")
        i = 0
        while i < len(params):
            p = params[i]
            if p in (b"38", b"48", b"58") and i + 1 < len(params):
                if params[i + 1] == b"2":
                    tc += 1
                    i += 5
                    continue
                if params[i + 1] == b"5":
                    c256 += 1
                    i += 3
                    continue
            try:
                v = int(p) if p else 0
            except ValueError:
                v = -1
            if 30 <= v <= 37 or 40 <= v <= 47 or 90 <= v <= 97 or 100 <= v <= 107:
                basic += 1
            elif v not in (0, 39, 49):
                other += 1
            i += 1
    return {"truecolor": tc, "c256": c256, "basic16": basic, "other_attrs": other}


def seq_counts(data: bytes) -> dict:
    return {
        "clear_2J": data.count(b"\x1b[2J"),
        "clear_3J": data.count(b"\x1b[3J"),
        "RIS_esc_c": len(re.findall(rb"\x1bc", data)),
        "erase_below_J": len(re.findall(rb"\x1b\[0?J", data)),
        "erase_line_K": len(re.findall(rb"\x1b\[[012]?K", data)),
        "cup_moves": len(re.findall(rb"\x1b\[\d*;?\d*[Hf]", data)),
        "alt_enter": data.count(b"\x1b[?1049h") + data.count(b"\x1b[?47h") + data.count(b"\x1b[?1047h"),
        "alt_exit": data.count(b"\x1b[?1049l") + data.count(b"\x1b[?47l") + data.count(b"\x1b[?1047l"),
        "cursor_hide": data.count(b"\x1b[?25l"),
        "cursor_show": data.count(b"\x1b[?25h"),
        "sync_update": data.count(b"\x1b[?2026h"),
        "mouse_enable": len(re.findall(rb"\x1b\[\?(1000|1002|1003|1006)h", data)),
    }


def restore_check(tail: bytes) -> dict:
    return {
        "alt_screen_exit": (b"\x1b[?1049l" in tail) or (b"\x1b[?47l" in tail) or (b"\x1b[?1047l" in tail),
        "cursor_show": b"\x1b[?25h" in tail,
        "sgr_reset": bool(re.search(rb"\x1b\[0?m", tail)),
    }


def termios_summary(t) -> dict:
    iflag, oflag, cflag, lflag = t[0], t[1], t[2], t[3]
    return {
        "ICANON": bool(lflag & termios.ICANON), "ECHO": bool(lflag & termios.ECHO),
        "ISIG": bool(lflag & termios.ISIG), "IEXTEN": bool(lflag & termios.IEXTEN),
        "ICRNL": bool(iflag & termios.ICRNL), "IXON": bool(iflag & termios.IXON),
        "OPOST": bool(oflag & termios.OPOST),
        "raw": [iflag, oflag, cflag, lflag],
    }


# ---------------------------------------------------------------- live metric references

def _read(path):
    try:
        with open(path) as fh:
            return fh.read()
    except OSError:
        return None


def linux_reference() -> dict:
    ref: dict = {"platform": "Linux", "t_wall": time.strftime("%Y-%m-%d %H:%M:%S"),
                 "hostname": socket.gethostname()}
    la = (_read("/proc/loadavg") or "").split()
    ref["loadavg"] = [float(x) for x in la[:3]] if la else None
    up = (_read("/proc/uptime") or "0").split()
    ref["uptime_s"] = float(up[0]) if up else None
    mem = {}
    for line in (_read("/proc/meminfo") or "").splitlines():
        k, _, v = line.partition(":")
        mem[k] = int(v.split()[0]) if v.split() else 0
    ref["mem_kib"] = {k: mem.get(k) for k in ("MemTotal", "MemAvailable", "MemFree", "Buffers", "Cached",
                                                "SwapTotal", "SwapFree")}
    if mem.get("MemTotal"):
        ref["mem_used_kib_total_minus_avail"] = mem["MemTotal"] - mem.get("MemAvailable", 0)
        ref["mem_total_gib"] = round(mem["MemTotal"] / 1048576, 2)
        ref["mem_avail_gib"] = round(mem.get("MemAvailable", 0) / 1048576, 2)
        ref["mem_used_gib"] = round(ref["mem_used_kib_total_minus_avail"] / 1048576, 2)
        ref["swap_total_gib"] = round(mem.get("SwapTotal", 0) / 1048576, 2)
        ref["swap_used_gib"] = round((mem.get("SwapTotal", 0) - mem.get("SwapFree", 0)) / 1048576, 2)
    st = os.statvfs("/")
    total = st.f_blocks * st.f_frsize
    avail = st.f_bavail * st.f_frsize
    used = (st.f_blocks - st.f_bfree) * st.f_frsize
    ref["root_disk"] = {"total_gib": round(total / 2**30, 1), "used_gib": round(used / 2**30, 1),
                        "avail_gib": round(avail / 2**30, 1),
                        "used_pct_df": round(100 * used / max(1, used + avail), 1),
                        "used_pct_of_total": round(100 * used / max(1, total), 1)}
    cpus = [l for l in (_read("/proc/stat") or "").splitlines() if re.match(r"cpu\d+ ", l)]
    ref["ncpu"] = len(cpus)
    ref["proc_stat_cpu"] = (_read("/proc/stat") or "").splitlines()[0]
    net = {}
    for line in (_read("/proc/net/dev") or "").splitlines()[2:]:
        name, _, rest = line.partition(":")
        f = rest.split()
        if len(f) >= 9:
            net[name.strip()] = {"rx": int(f[0]), "tx": int(f[8])}
    ref["net_dev"] = net
    pids = [p for p in os.listdir("/proc") if p.isdigit()]
    ref["process_count"] = len(pids)
    ref["os"] = f"{platform.system()} {platform.release()} {platform.machine()}"
    return ref


def linux_cpu_sample():
    """(total_jiffies, idle_jiffies, {pid: (name, ticks, rss_kib)})"""
    line = (_read("/proc/stat") or "").splitlines()[0].split()[1:]
    vals = [int(x) for x in line]
    idle = vals[3] + (vals[4] if len(vals) > 4 else 0)
    procs = {}
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        s = _read(f"/proc/{p}/stat")
        if not s:
            continue
        try:
            name = s[s.index("(") + 1:s.rindex(")")]
            rest = s[s.rindex(")") + 2:].split()
            ticks = int(rest[11]) + int(rest[12])
            rss = int(rest[21]) * (os.sysconf("SC_PAGE_SIZE") // 1024)
            procs[int(p)] = (name, ticks, rss)
        except (ValueError, IndexError):
            continue
    return sum(vals), idle, procs


def linux_reference_with_rates(window=1.0) -> dict:
    """Reference snapshot plus CPU%, net rates and top-5 processes over `window` seconds."""
    n0 = linux_reference()["net_dev"]
    tot0, idle0, p0 = linux_cpu_sample()
    t0 = time.monotonic()
    time.sleep(window)
    ref = linux_reference()
    tot1, idle1, p1 = linux_cpu_sample()
    dt = time.monotonic() - t0
    ref["cpu_pct_overall"] = round(100 * (1 - (idle1 - idle0) / max(1, tot1 - tot0)), 1)
    rx = sum(v["rx"] - n0.get(k, v)["rx"] for k, v in ref["net_dev"].items() if k != "lo")
    tx = sum(v["tx"] - n0.get(k, v)["tx"] for k, v in ref["net_dev"].items() if k != "lo")
    ref["net_rate_non_lo_Bps"] = {"rx": round(rx / dt), "tx": round(tx / dt)}
    hz = os.sysconf("SC_CLK_TCK")
    top = []
    for pid, (name, ticks, rss) in p1.items():
        if pid in p0:
            top.append((round(100 * (ticks - p0[pid][1]) / hz / dt, 1), pid, name, rss))
    top.sort(reverse=True)
    ref["top_cpu_processes"] = [{"cpu_pct": c, "pid": p, "name": n, "rss_kib": r} for c, p, n, r in top[:8]]
    return ref


def darwin_reference() -> dict:
    """macOS reference values (evaluator-side; shelling out is fine for the *reference*)."""
    def sh(cmd):
        try:
            return subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=10).stdout
        except Exception:
            return ""
    ref = {"platform": "Darwin", "t_wall": time.strftime("%Y-%m-%d %H:%M:%S"),
           "hostname": socket.gethostname()}
    ref["loadavg"] = sh("sysctl -n vm.loadavg").strip()
    ref["boottime"] = sh("sysctl -n kern.boottime").strip()
    ref["uptime"] = sh("uptime").strip()
    ref["memsize"] = sh("sysctl -n hw.memsize").strip()
    ref["ncpu"] = sh("sysctl -n hw.ncpu").strip()
    ref["swapusage"] = sh("sysctl -n vm.swapusage").strip()
    ref["vm_stat"] = sh("vm_stat").strip()
    ref["df_root"] = sh("df -k /").strip()
    ref["process_count"] = sh("ps -A -o pid= | wc -l").strip()
    ref["top"] = sh("top -l 2 -n 8 -o cpu -stats pid,command,cpu,mem -s 1 | tail -12").strip()
    ref["netstat_ib"] = sh("netstat -ib | head -20").strip()
    ref["os"] = sh("sw_vers -productVersion").strip() + " " + platform.machine()
    return ref


def reference(rates=True) -> dict:
    if platform.system() == "Linux":
        return linux_reference_with_rates() if rates else linux_reference()
    return darwin_reference()


def dump_json(obj, path):
    with open(path, "w") as fh:
        json.dump(obj, fh, indent=2, default=str)
