#!/usr/bin/env python3
"""PTY lifecycle checks and measured startup, output, CPU, RSS, and binary size.

Python standard library only. CPU is process time / measured elapsed wall time;
100% means one core. Live RSS is sampled from /proc or libproc; getrusage's
launcher-inclusive lifetime high-water mark is also recorded separately.
"""
import argparse
import ctypes
import fcntl
import json
import os
from pathlib import Path
import platform
import pty
import select
import signal
import statistics
import struct
import subprocess
import termios
import time


def resident_bytes(pid):
    """Sample the running executable, avoiding the launcher's inherited RSS peak."""
    if platform.system() == "Linux":
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1])*1024
    elif platform.system() == "Darwin":
        library = ctypes.CDLL("/usr/lib/libproc.dylib")
        library.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
        library.proc_pidinfo.restype = ctypes.c_int
        # Public proc_taskinfo: six uint64 values followed by twelve int32 values.
        data = ctypes.create_string_buffer(96)
        if library.proc_pidinfo(pid, 4, 0, data, len(data)) == len(data):
            return struct.unpack_from("=Q", data.raw, 8)[0]
    return None


class Terminal:
    def __init__(self, binary, extra=(), color=True, controlling=True):
        self.master, self.slave = pty.openpty()
        self.resize(120, 36)
        self.original = termios.tcgetattr(self.slave)
        self.output = bytearray()
        env = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor", LANG="C.UTF-8")
        if not color:
            env["NO_COLOR"] = "1"
        def child_setup():
            os.setsid()
            fcntl.ioctl(self.slave, termios.TIOCSCTTY, 0)
        self.start = time.perf_counter()
        self.process = subprocess.Popen([str(binary), *extra], stdin=self.slave, stdout=self.slave,
                                        stderr=self.slave, env=env,
                                        preexec_fn=child_setup if controlling else None,
                                        start_new_session=not controlling)

    def resize(self, w, h):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", h, w, 0, 0))

    def read(self, seconds):
        end = time.perf_counter() + seconds
        start = len(self.output)
        while time.perf_counter() < end:
            ready, _, _ = select.select([self.master], [], [], max(0, end-time.perf_counter()))
            if ready:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.output.extend(data)
        return bytes(self.output[start:])

    def expect(self, value, timeout=3):
        end = time.perf_counter() + timeout
        while value not in self.output:
            self.read(min(0.005, max(0, end-time.perf_counter())))
            if time.perf_counter() >= end:
                raise AssertionError(f"missing {value!r}; output tail: {self.output[-300:]!r}")

    def send(self, data):
        os.write(self.master, data)

    def finish(self, data=b"q", sig=None):
        if sig is None:
            self.send(data)
        else:
            os.kill(self.process.pid, sig)
        self.read(0.08)
        deadline = time.perf_counter()+3
        while True:
            pid, status, usage = os.wait4(self.process.pid, os.WNOHANG)
            if pid:
                break
            if time.perf_counter() > deadline:
                self.process.kill()
                raise AssertionError("terminal process did not exit")
            self.read(0.01)
        self.process.returncode = os.waitstatus_to_exitcode(status)
        self.read(0.01)
        assert self.process.returncode == 0, self.output[-1000:]
        assert termios.tcgetattr(self.slave) == self.original, "input mode was not restored"
        assert b"\x1b[?25h" in self.output and b"\x1b[?1049l" in self.output, "cursor/alternate screen not restored"
        os.close(self.master)
        os.close(self.slave)
        return usage


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("bin/lcars"))
    parser.add_argument("--seconds", type=float, default=12)
    parser.add_argument("--output", type=Path, default=Path("artifacts/performance.json"))
    args = parser.parse_args()
    binary = args.binary.resolve()
    startup = []
    for _ in range(10):
        terminal = Terminal(binary, controlling=False)
        # The footer precedes telemetry in the paint order but confirms a full
        # initial composition, beyond just entering the alternate screen.
        terminal.expect(b"[q] exit")
        startup.append((time.perf_counter()-terminal.start)*1000)
        terminal.finish()

    terminal = Terminal(binary)
    terminal.expect(b"CHRONOMETRY")
    assert b"FIVE MOST ACTIVE" not in terminal.output, "processes leaked into command view"
    terminal.send(b"p")
    terminal.expect(b"FIVE MOST ACTIVE")
    terminal.send(b"?")
    terminal.expect(b"OPERATOR REFERENCE")
    terminal.send(b"?")
    terminal.send(b"\t")
    terminal.read(0.1)
    terminal.send(b" ")
    terminal.read(0.15)
    assert terminal.read(0.6) == b"", "paused display still updates"
    terminal.send(b" ")
    assert terminal.read(0.6), "resume did not repaint"
    terminal.send(b"\x1b[A")
    terminal.read(0.1)
    assert terminal.process.poll() is None, "arrow key was mistaken for Escape"
    for w, h in [(80, 24), (60, 20), (10, 3), (1, 1), (160, 50), (120, 36)]:
        terminal.resize(w, h)
        os.kill(terminal.process.pid, signal.SIGWINCH)
        terminal.read(0.07)
        assert terminal.process.poll() is None, f"failed at {w}x{h}"
    assert b"\x1b[2J" not in terminal.output, "refresh clears the screen"
    terminal.finish()

    for key, sig in [(b"\x1b", None), (b"\x03", None), (None, signal.SIGTERM),
                     (None, signal.SIGHUP), (None, signal.SIGQUIT)]:
        terminal = Terminal(binary)
        terminal.expect(b"CHRONOMETRY")
        terminal.finish(key, sig)

    terminal = Terminal(binary, ["--ascii", "--color", "mono"], color=False)
    terminal.read(1.1)
    terminal.finish()
    assert all(byte < 128 for byte in terminal.output), "ASCII mode emitted Unicode"
    assert b"38;2" not in terminal.output and b"38;5" not in terminal.output, "mono emitted color"

    redirected = subprocess.run([str(binary)], input=b"", capture_output=True,
                                env=dict(os.environ, TERM="xterm-256color"))
    assert redirected.returncode == 1 and b"--snapshot" in redirected.stderr and not redirected.stdout
    unsuitable = subprocess.run([str(binary)], capture_output=True, env=dict(os.environ, TERM="dumb"))
    assert unsuitable.returncode == 1 and not unsuitable.stdout
    snapshot = subprocess.run([str(binary), "--snapshot", "--ascii", "--size", "80x24"], capture_output=True)
    assert snapshot.returncode == 0 and len(snapshot.stdout.splitlines()) == 24 and b"\x1b" not in snapshot.stdout
    for flags in [["--color", "invalid"], ["--interval", "0s"], ["--snapshot", "--size", "0x0"], ["--view", "invalid"]]:
        invalid = subprocess.run([str(binary), *flags], capture_output=True)
        assert invalid.returncode == 1 and not invalid.stdout and invalid.stderr

    profiles = {}
    for view in ["command", "diagnostics"]:
        # Avoid executing Python in the child before exec, which can inflate the
        # child's high-water RSS. Lifecycle cases above do acquire a controlling TTY.
        terminal = Terminal(binary, ["--view", view], controlling=False)
        terminal.expect(b"[q] exit")
        initial_bytes = len(terminal.output)
        measured_start = time.perf_counter()
        steady_bytes, sampled_rss = 0, []
        while time.perf_counter()-measured_start < args.seconds:
            steady_bytes += len(terminal.read(min(0.1, max(0, args.seconds-(time.perf_counter()-measured_start)))))
            rss = resident_bytes(terminal.process.pid)
            if rss is not None:
                sampled_rss.append(rss)
        elapsed = time.perf_counter()-measured_start
        usage = terminal.finish()
        cpu = usage.ru_utime + usage.ru_stime
        rss = usage.ru_maxrss if platform.system() == "Darwin" else usage.ru_maxrss*1024
        profiles[view] = {"wall_seconds": round(elapsed, 3), "cpu_seconds": round(cpu, 6),
                          "cpu_percent_one_core": round(cpu/elapsed*100, 3),
                          "peak_sampled_rss_bytes": max(sampled_rss) if sampled_rss else None,
                          "lifetime_peak_rss_including_launcher_bytes": rss,
                          "initial_output_bytes": initial_bytes,
                          "steady_output_bytes_per_second": round(steady_bytes/elapsed, 1)}
    result = {"platform": platform.platform(), "python": platform.python_version(),
              "binary_bytes": binary.stat().st_size, "startup_trials": len(startup),
              "startup_median_ms": round(statistics.median(startup), 3),
              "startup_max_ms": round(max(startup), 3), "startup_samples_ms": [round(s, 3) for s in startup],
              "profiles": profiles, "terminal_checks": "passed"}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+"\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
