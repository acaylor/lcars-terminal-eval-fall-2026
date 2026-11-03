#!/usr/bin/env python3
"""Interactive lifecycle smoke test using only Python's standard library."""

import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

BINARY = sys.argv[1] if len(sys.argv) > 1 else "target/release/lcars-monitor"


def read_available(process, fd, duration=0.35):
    output = bytearray()
    deadline = time.monotonic() + duration
    while time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], max(0, deadline - time.monotonic()))
        if not ready:
            break
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        output.extend(chunk)
    return bytes(output)


def launch():
    master, slave = pty.openpty()
    original = termios.tcgetattr(slave)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))
    process = subprocess.Popen(
        [BINARY], stdin=slave, stdout=slave, stderr=slave, close_fds=True
    )
    os.close(slave)
    return process, master, original


def finish(process, master, original, output):
    process.wait(timeout=3)
    restored = termios.tcgetattr(master) == original
    os.close(master)
    assert process.returncode == 0, f"exit status {process.returncode}: {output!r}"
    assert b"\x1b[?1049l" in output, "alternate screen was not restored"
    assert b"\x1b[?25h" in output, "cursor visibility was not restored"
    assert restored, "terminal input mode was not restored"


def controls_and_quit():
    process, master, original = launch()
    output = read_available(process, master, 0.8)
    assert b"ENGINEERING STATUS" in output
    assert b"TOP CPU PROCESSES" not in output
    os.write(master, b"p")
    output += read_available(process, master)
    assert b"TOP CPU PROCESSES" in output
    os.write(master, b"\t")
    output += read_available(process, master)
    os.write(master, b"?")
    output += read_available(process, master)
    assert b"COMMAND REFERENCE" in output
    os.write(master, b"? ")
    output += read_available(process, master)
    assert b"PAUSED" in output
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    output += read_available(process, master)
    os.write(master, b"q")
    output += read_available(process, master)
    finish(process, master, original, output)


def interrupted(key):
    process, master, original = launch()
    output = read_available(process, master, 0.5)
    if key == "signal":
        process.send_signal(signal.SIGTERM)
    else:
        os.write(master, b"\x03")
    output += read_available(process, master)
    finish(process, master, original, output)


controls_and_quit()
interrupted("ctrl-c")
interrupted("signal")
print("interactive smoke: controls, resize, q, Ctrl-C, SIGTERM, and restoration passed")
