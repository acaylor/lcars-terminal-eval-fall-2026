#!/usr/bin/env python3
"""Measure actual PTY startup, steady CPU/RSS, diff output and terminal cleanup.

Uses the Python standard library. No privileged operations or external packages.
Linux reports current RSS from /proc; macOS uses RUSAGE_CHILDREN peak RSS.
"""
import argparse
import fcntl
import json
import os
import platform
import pty
import resource
import select
import signal
import statistics
import struct
import subprocess
import termios
import time


def drain(master, duration):
    data = bytearray()
    deadline = time.monotonic() + duration
    while time.monotonic() < deadline:
        ready, _, _ = select.select([master], [], [], min(0.05, max(0, deadline-time.monotonic())))
        if ready:
            try:
                data.extend(os.read(master, 65536))
            except OSError:
                break
    return bytes(data)


def launch(binary, width=120, height=36, extra=()):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
    before = termios.tcgetattr(slave)
    env = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor")
    start = time.perf_counter()
    def attach_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    child = subprocess.Popen([binary, *extra], stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=attach_terminal)
    data = bytearray()
    while b"\x1b[0m" not in data:
        ready, _, _ = select.select([master], [], [], 0.05)
        if ready:
            data.extend(os.read(master, 65536))
        if child.poll() is not None:
            raise RuntimeError(f"application exited at startup: {data!r}")
        if time.perf_counter()-start > 5:
            child.kill()
            raise RuntimeError("no first frame within five seconds")
    startup_ms = (time.perf_counter()-start)*1000
    return child, master, slave, before, startup_ms, bytes(data)


def stop(child, master, slave, before, sig=None):
    if sig is None:
        os.write(master, b"q")
    else:
        child.send_signal(sig)
    final = drain(master, 0.15)
    child.wait(timeout=3)
    if termios.tcgetattr(slave) != before:
        raise AssertionError("terminal input mode was not restored")
    for required in (b"\x1b[?25h", b"\x1b[?1049l", b"\x1b[?7h"):
        if required not in final:
            raise AssertionError(f"missing terminal restoration {required!r}")
    if child.returncode != 0:
        raise AssertionError(f"unexpected exit code {child.returncode}: {final!r}")
    os.close(master)
    os.close(slave)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", default="./bin/lcars")
    parser.add_argument("--seconds", type=float, default=6, help="idle measurement duration")
    args = parser.parse_args()
    if args.seconds <= 0:
        parser.error("--seconds must be positive")
    startup = []
    for _ in range(10):
        child, master, slave, before, ms, _ = launch(args.binary)
        startup.append(ms)
        stop(child, master, slave, before)
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    child, master, slave, before, _, first = launch(args.binary)
    idle_start = time.monotonic()
    frames = drain(master, args.seconds)
    idle_elapsed = time.monotonic()-idle_start
    rss = None
    if platform.system() == "Linux":
        with open(f"/proc/{child.pid}/status", encoding="utf8") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    rss = int(line.split()[1])*1024
    stop(child, master, slave, before)
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu_seconds = (usage_after.ru_utime+usage_after.ru_stime)-(usage_before.ru_utime+usage_before.ru_stime)
    if rss is None:
        rss = usage_after.ru_maxrss  # Darwin reports bytes; a peak across children.
    # Full repaint is permitted on resize, never on steady sample refreshes.
    assert b"\x1b[2J" not in frames, "full clear during normal updates"
    child, master, slave, before, _, _ = launch(args.binary)
    for key, label in ((b"?", b"COMMAND REFERENCE"), (b"?p", b"PROCESS ACTIVITY"), (b"\t", b"CHRONOMETRY")):
        os.write(master, key)
        assert label in drain(master, 0.2), f"key did not reveal {label!r}"
    os.write(master, b" ")
    drain(master, 0.15)
    assert drain(master, 1.2) == b"", "paused screen continued to update"
    os.write(master, b" ")
    drain(master, 0.15)
    for width, height in ((80, 24), (20, 5), (1, 1), (120, 36)):
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        child.send_signal(signal.SIGWINCH)
        assert drain(master, 0.15), "resize did not redraw"
        assert child.poll() is None, "resize crashed the application"
    stop(child, master, slave, before, signal.SIGTERM)
    for sig in (signal.SIGINT, signal.SIGHUP, signal.SIGQUIT):
        child, master, slave, before, _, _ = launch(args.binary, extra=("--ascii", "--color", "mono"))
        stop(child, master, slave, before, sig)
    for key in (b"\x1b", b"\x03"):
        child, master, slave, before, _, _ = launch(args.binary)
        os.write(master, key)
        final = drain(master, 0.2)
        child.wait(timeout=3)
        assert termios.tcgetattr(slave) == before, "input mode not restored by Esc/Ctrl-C"
        assert b"\x1b[?25h" in final and b"\x1b[?1049l" in final, "missing cursor/screen cleanup"
        os.close(master)
        os.close(slave)
    redirected = subprocess.run([args.binary], input=b"", capture_output=True, env=dict(os.environ, TERM="xterm"))
    assert redirected.returncode == 1 and b"--snapshot" in redirected.stderr, "redirected output not rejected"
    snapshot = subprocess.run([args.binary, "--snapshot", "--ascii", "--width", "80", "--height", "24"], capture_output=True)
    assert snapshot.returncode == 0 and b"\x1b" not in snapshot.stdout
    assert len(snapshot.stdout.splitlines()) == 24
    results = {
        "platform": platform.platform(),
        "startup_trials": len(startup),
        "startup_median_ms": round(statistics.median(startup), 3),
        "startup_max_ms": round(max(startup), 3),
        "executable_bytes": os.path.getsize(args.binary),
        "idle_sample_seconds": round(idle_elapsed, 3),
        "process_cpu_seconds_including_startup_exit": round(cpu_seconds, 6),
        "cpu_percent_one_core": round(cpu_seconds/idle_elapsed*100, 3),
        "rss_bytes": rss,
        "rss_method": "live /proc VmRSS" if platform.system() == "Linux" else "RUSAGE_CHILDREN peak across runs",
        "first_frame_bytes": len(first),
        "idle_output_bytes": len(frames),
        "pty_checks": "PASS: q, Esc, Ctrl-C, SIGINT/TERM/HUP/QUIT, help, pause, p, Tab, resize, ASCII/mono, redirected output",
    }
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
