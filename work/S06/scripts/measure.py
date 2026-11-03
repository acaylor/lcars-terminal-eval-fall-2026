#!/usr/bin/env python3
"""Measure interactive startup, idle CPU, peak RSS, and executable size."""

import fcntl
import os
import pty
import resource
import select
import struct
import subprocess
import sys
import termios
import time

binary = sys.argv[1] if len(sys.argv) > 1 else "target/release/lcars-monitor"
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))
before = resource.getrusage(resource.RUSAGE_CHILDREN)
started = time.perf_counter()
process = subprocess.Popen([binary], stdin=slave, stdout=slave, stderr=slave)
os.close(slave)
ready, _, _ = select.select([master], [], [], 5)
if not ready:
    process.kill()
    raise SystemExit("no interactive frame within five seconds")
os.read(master, 65536)
startup = time.perf_counter() - started
measurement_start = time.perf_counter()
time.sleep(5)
os.write(master, b"q")
process.wait(timeout=3)
elapsed = time.perf_counter() - measurement_start
after = resource.getrusage(resource.RUSAGE_CHILDREN)
cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
rss = after.ru_maxrss * (1 if sys.platform == "darwin" else 1024)
os.close(master)
print(f"interactive startup: {startup * 1000:.1f} ms")
print(f"five-second idle CPU: {cpu / elapsed * 100:.2f}% of one core")
print(f"peak RSS: {rss / 1024 / 1024:.2f} MiB")
print(f"executable size: {os.path.getsize(binary)} bytes")
