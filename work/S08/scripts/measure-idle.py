#!/usr/bin/env python3
"""Measure idle CPU, RSS, and terminal output rate while the TUI is live.

Linux: reads /proc. macOS: use `ps -o %cpu,rss -p PID` instead.
"""
import fcntl
import os
import pty
import select
import struct
import sys
import termios
import time

BIN = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "target/release/lcars-mon")
SECONDS = float(sys.argv[2]) if len(sys.argv) > 2 else 8.0
HZ = os.sysconf("SC_CLK_TCK")


def ticks(pid):
    with open(f"/proc/{pid}/stat") as f:
        fields = f.read().split()
    return int(fields[13]) + int(fields[14])


def rss_kb(pid):
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    return 0


pid, fd = pty.fork()
if pid == 0:
    env = os.environ.copy()
    env.update(TERM="xterm-256color", COLORTERM="truecolor", LANG="C.UTF-8")
    os.execve(BIN, [BIN], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))

buf = b""
start = time.perf_counter()
while time.perf_counter() - start < 5 and b"LOCAL TIME" not in buf:
    r, _, _ = select.select([fd], [], [], 0.05)
    if r:
        try:
            buf = (buf + os.read(fd, 65536))[-8192:]
        except OSError:
            break

time.sleep(2.0)
t0 = ticks(pid)
b0 = 0
start = time.perf_counter()
while time.perf_counter() - start < SECONDS:
    r, _, _ = select.select([fd], [], [], 0.05)
    if r:
        try:
            b0 += len(os.read(fd, 65536))
        except OSError:
            break
elapsed = time.perf_counter() - start
t1 = ticks(pid)
rss = rss_kb(pid)
try:
    os.write(fd, b"q")
except OSError:
    pass
try:
    os.waitpid(pid, 0)
except ChildProcessError:
    pass

cpu = ((t1 - t0) / HZ) / elapsed * 100.0
print(f"window: {elapsed:.2f}s")
print(f"idle CPU: {cpu:.2f}% of one core")
print(f"RSS: {rss} KiB")
print(f"terminal output: {b0/elapsed:.0f} bytes/s")
