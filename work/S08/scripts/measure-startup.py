#!/usr/bin/env python3
"""Measure time from process spawn to the first rendered LCARS frame.

Uses a real PTY at 120x36 so terminal setup is included. Linux/macOS.
"""
import fcntl
import os
import pty
import select
import signal
import statistics
import struct
import sys
import termios
import time

BIN = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "target/release/lcars-mon")
RUNS = int(sys.argv[2]) if len(sys.argv) > 2 else 5


def once():
    pid, fd = pty.fork()
    if pid == 0:
        env = os.environ.copy()
        env.update(TERM="xterm-256color", COLORTERM="truecolor", LANG="C.UTF-8")
        os.execve(BIN, [BIN], env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))
    started = time.perf_counter()
    first = None
    buf = b""
    while time.perf_counter() - started < 5.0:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            buf += chunk
            if first is None and b"LOCAL TIME" in buf:
                first = (time.perf_counter() - started) * 1000.0
                break
    try:
        os.write(fd, b"q")
    except OSError:
        pass
    try:
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass
    if first is None:
        os.kill(pid, signal.SIGKILL)
        raise SystemExit("first frame not observed")
    return first


values = [once() for _ in range(RUNS)]
print(f"runs: {RUNS}")
for i, v in enumerate(values, 1):
    print(f"  run {i}: {v:.1f} ms")
print(f"min {min(values):.1f} ms  median {statistics.median(values):.1f} ms  max {max(values):.1f} ms")
