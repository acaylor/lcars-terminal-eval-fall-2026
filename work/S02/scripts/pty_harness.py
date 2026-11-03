#!/usr/bin/env python3
"""Drive the real lcarsmon binary inside a pseudo-terminal.

Used for three things that unit tests cannot cover:

  lifecycle  - verify the terminal (termios flags, cursor, alt screen) is
               restored after q, Esc, Ctrl-C, SIGTERM, SIGHUP, and that the
               app survives resizes and Ctrl-Z/fg.
  capture    - run the interactive app for a while, feed its byte stream
               through a terminal emulator (pyte) and write the resulting
               screen as ANSI, so captures show the *real* diff renderer.
  startup    - time from spawn to the first complete frame on the pty.
  resources  - CPU time, peak RSS and terminal output of an N-second run
               (stdlib only; pyte is needed just for `capture`).

    uv run --with pyte scripts/pty_harness.py lifecycle ./target/release/lcarsmon
    uv run --with pyte scripts/pty_harness.py capture ./target/release/lcarsmon out.ansi 120x36 40 [keys]
    python3 scripts/pty_harness.py startup ./target/release/lcarsmon 20
    python3 scripts/pty_harness.py resources ./target/release/lcarsmon 30 command|diagnostic
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

ENV = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor", LANG="C.UTF-8")


def set_size(fd, cols, rows):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def spawn(binary, cols, rows, args=()):
    pid, master = pty.fork()
    if pid == 0:
        os.execve(binary, [binary, *args], ENV)
    set_size(master, cols, rows)
    return pid, master


def spawn_job(binary, cols, rows):
    """Like spawn(), but the app runs as a foreground job of a tiny shell
    that resumes it 0.4 s after it stops (like typing `fg`)."""
    pid, master = pty.fork()
    if pid == 0:
        child = os.fork()
        if child == 0:
            os.setpgid(0, 0)
            signal.signal(signal.SIGTTOU, signal.SIG_IGN)
            os.tcsetpgrp(0, os.getpgrp())
            signal.signal(signal.SIGTTOU, signal.SIG_DFL)
            os.execve(binary, [binary], ENV)
        os.setpgid(child, child)
        while True:
            _, status = os.waitpid(child, os.WUNTRACED)
            if os.WIFSTOPPED(status):
                os.write(1, b"JOB-STOPPED\r\n")
                time.sleep(0.4)
                signal.signal(signal.SIGTTOU, signal.SIG_IGN)
                os.tcsetpgrp(0, child)
                os.killpg(child, signal.SIGCONT)
                continue
            os._exit(os.waitstatus_to_exitcode(status) & 0xFF)
    set_size(master, cols, rows)
    return pid, master


def pump(master, seconds, sink=None):
    """Read output for `seconds`; returns bytes read."""
    out = bytearray()
    end = time.monotonic() + seconds
    while True:
        left = end - time.monotonic()
        if left <= 0:
            break
        r, _, _ = select.select([master], [], [], min(left, 0.05))
        if r:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            out += chunk
            if sink:
                sink(chunk)
    return bytes(out)


def wait_exit(pid, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            return os.waitstatus_to_exitcode(status)
        time.sleep(0.02)
    os.kill(pid, signal.SIGKILL)
    return None


def lifecycle(binary):
    results = []

    def scenario(name, action, job=False):
        pid, master = (spawn_job if job else spawn)(binary, 120, 36)
        pump(master, 0.8)
        # The child's pty starts cooked; after exit it must be cooked again.
        out = action(pid, master) or b""
        out += pump(master, 0.3)
        code = wait_exit(pid)
        out += pump(master, 0.2)
        try:
            attrs = termios.tcgetattr(master)
            lflag = attrs[3]
            cooked = bool(lflag & termios.ICANON) and bool(lflag & termios.ECHO)
        except termios.error:
            cooked = None
        restored = b"\x1b[?25h" in out and b"\x1b[?1049l" in out
        ok = code is not None and cooked is not False and restored
        results.append((name, ok, code, cooked, restored))
        os.close(master)

    def key(b):
        return lambda pid, m: os.write(m, b) and None

    def sig(s):
        return lambda pid, m: os.kill(pid, s)

    def resize_then_quit(pid, m):
        out = b""
        for cols, rows in [(80, 24), (30, 8), (200, 60), (1, 1), (120, 36)]:
            set_size(m, cols, rows)
            os.kill(pid, signal.SIGWINCH)
            out += pump(m, 0.3)
        os.write(m, b"p")
        out += pump(m, 1.2)
        os.write(m, b"?\t \x1b[A")
        out += pump(m, 0.4)
        os.write(m, b"q")
        return out

    def suspend_resume(pid, m):
        # pty.fork() makes the app a session leader, whose process group is
        # orphaned, and the kernel ignores SIGTSTP for orphaned groups. This
        # scenario therefore runs the app under a minimal job-control "shell"
        # (see spawn_job) which prints JOB-STOPPED when the app stops itself.
        os.write(m, b"\x1a")
        out = pump(m, 1.2)
        os.write(m, b"q")
        out += pump(m, 0.3)
        if b"JOB-STOPPED" not in out:
            raise SystemExit("Ctrl-Z did not stop the process")
        # Terminal must be restored *before* stopping, then re-entered.
        stop_at = out.index(b"JOB-STOPPED")
        if b"\x1b[?1049l" not in out[:stop_at] or b"\x1b[?1049h" not in out[stop_at:]:
            raise SystemExit("terminal not restored around suspend")
        return out

    scenario("quit with q", key(b"q"))
    scenario("quit with Esc", key(b"\x1b"))
    scenario("Ctrl-C", key(b"\x03"))
    scenario("SIGTERM", sig(signal.SIGTERM))
    scenario("SIGINT", sig(signal.SIGINT))
    scenario("SIGHUP", sig(signal.SIGHUP))
    scenario("resize storm + keys", resize_then_quit)
    scenario("Ctrl-Z then fg", suspend_resume, job=True)

    width = max(len(r[0]) for r in results)
    failed = 0
    for name, ok, code, cooked, restored in results:
        failed += not ok
        print(f"{'PASS' if ok else 'FAIL'}  {name:<{width}}  exit={code} cooked={cooked} restore-seq={restored}")
    return 1 if failed else 0


def screen_to_ansi(screen):
    import pyte  # noqa: F401

    def rgb(c, default):
        if c == "default" or not c:
            return default
        if len(c) == 6:
            return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))
        named = {"black": (0, 0, 0), "white": (229, 229, 229)}
        return named.get(c, default)

    lines = []
    for y in range(screen.lines):
        row = screen.buffer[y]
        parts = []
        for x in range(screen.columns):
            ch = row[x]
            if ch.data == "":
                continue  # right half of a wide char
            fg = rgb(ch.fg, (230, 230, 230))
            bg = rgb(ch.bg, (0, 0, 0))
            if ch.reverse:
                fg, bg = bg, fg
            bold = ";1" if ch.bold else ""
            parts.append(f"\x1b[0{bold};38;2;{fg[0]};{fg[1]};{fg[2]};48;2;{bg[0]};{bg[1]};{bg[2]}m{ch.data}")
        lines.append("".join(parts) + "\x1b[0m")
    return "\n".join(lines) + "\n"


def capture(binary, dest, size, seconds, keys=""):
    """Run for `seconds`, save <dest>-0.ansi, then after each comma-separated
    key save <dest>-<n>.ansi (keys: literal chars, or space/tab/esc)."""
    import pyte

    cols, rows = (int(v) for v in size.split("x"))
    screen = pyte.Screen(cols, rows)
    stream = pyte.ByteStream(screen)
    pid, master = spawn(binary, cols, rows)
    total = pump(master, float(seconds), stream.feed)
    shots = [f"{dest}-0.ansi"]
    with open(shots[0], "w", encoding="utf-8") as f:
        f.write(screen_to_ansi(screen))
    for n, k in enumerate([k for k in keys.split(",") if k], start=1):
        os.write(master, {"space": b" ", "tab": b"\t", "esc": b"\x1b"}.get(k, k.encode()))
        total += pump(master, 2.2, stream.feed)
        shots.append(f"{dest}-{n}.ansi")
        with open(shots[-1], "w", encoding="utf-8") as f:
            f.write(screen_to_ansi(screen))
    os.write(master, b"q")
    pump(master, 0.3, stream.feed)
    wait_exit(pid)
    elapsed = float(seconds) + 2.2 * (len(shots) - 1)
    print(f"captured {', '.join(shots)} at {cols}x{rows}; {len(total)} bytes of terminal output "
          f"in {elapsed:.0f}s ({len(total) / elapsed:.0f} B/s average, including the first full paint)")
    return 0


def startup(binary, runs):
    times = []
    for _ in range(int(runs)):
        t0 = time.perf_counter()
        pid, master = spawn(binary, 120, 36)
        buf = b""
        # The first frame ends with the synchronized-output end marker.
        while b"\x1b[?2026l" not in buf:
            r, _, _ = select.select([master], [], [], 2.0)
            if not r:
                break
            buf += os.read(master, 65536)
        times.append((time.perf_counter() - t0) * 1000)
        os.write(master, b"q")
        pump(master, 0.2)
        wait_exit(pid)
        os.close(master)
    print(f"time to first full frame over {runs} runs: median {statistics.median(times):.1f} ms, "
          f"min {min(times):.1f} ms, max {max(times):.1f} ms")
    return 0


def rss_kib(pid):
    """Peak (Linux VmHWM) or current (macOS ps) resident size of the app.
    wait4()'s ru_maxrss can't be used: it includes the pre-exec Python fork."""
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmHWM:"):
                    return int(line.split()[1])
    except OSError:
        pass
    import subprocess
    out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    return int(out) if out else 0


def resources(binary, seconds, view="command"):
    seconds = float(seconds)
    pid, master = spawn(binary, 120, 36, ["--exit-after", str(int(seconds * 1000))])
    if view == "diagnostic":
        os.write(master, b"p")
    warm = pump(master, 1.0)
    steady = bytearray()
    peak = 0
    end = time.monotonic() + seconds - 1.5
    while time.monotonic() < end:
        steady += pump(master, 1.0)
        peak = max(peak, rss_kib(pid))
    steady += pump(master, 3.0)
    _, status, ru = os.wait4(pid, 0)
    cpu = ru.ru_utime + ru.ru_stime
    steady_secs = max(seconds - 1.0, 1.0)
    print(f"{view} view, {seconds:.0f}s run: CPU {cpu:.3f}s total = {cpu / seconds * 100:.2f}% of one core; "
          f"peak RSS {peak / 1024:.1f} MiB; output {len(warm)} B in the first second, "
          f"then {len(steady) / steady_secs:.0f} B/s")
    os.close(master)
    return 0


if __name__ == "__main__":
    cmd, *rest = sys.argv[1:]
    sys.exit({"lifecycle": lifecycle, "capture": capture, "startup": startup, "resources": resources}[cmd](*rest))
