#!/usr/bin/env python3
"""Linux performance & render-efficiency probe (run on a quiet host, one binary at a time).

Usage: perf_probe.py [--runs 15] [--long 0] S01 [S02 ...]
Writes artifacts/SXX/perf.json.

Measures, at 120x36 with COLORTERM=truecolor:
  * startup latency: exec -> first byte on the PTY (median of N runs), plus time until the
    first *substantial* frame (>= 30% of a full 120x36 screen's worth of printable cells)
  * steady state (2 s warmup, 8 s window): CPU % of one core from /proc/<pid>/stat ticks,
    VmRSS peak/mean, PTY bytes/sec, clear-screen sequences, frames (write bursts) per second
  * optional long run (--long N seconds) to check RSS growth
  * release binary size in bytes
"""
import argparse
import json
import os
import re
import statistics
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lcars_harness as H  # noqa: E402

HZ = os.sysconf("SC_CLK_TCK")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def proc_ticks(pid):
    with open(f"/proc/{pid}/stat") as fh:
        s = fh.read()
    rest = s[s.rindex(")") + 2:].split()
    return int(rest[11]) + int(rest[12])


def proc_rss_kib(pid):
    with open(f"/proc/{pid}/status") as fh:
        for line in fh:
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
            if line.startswith("VmHWM:"):
                pass
    return None


def proc_hwm_kib(pid):
    with open(f"/proc/{pid}/status") as fh:
        for line in fh:
            if line.startswith("VmHWM:"):
                return int(line.split()[1])
    return None


def threads(pid):
    try:
        return len(os.listdir(f"/proc/{pid}/task"))
    except OSError:
        return None


def printable_cells(data: bytes) -> int:
    txt = re.sub(rb"\x1b\[[0-9;:?<>=]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b[@-_]", b"", data)
    return len(txt.decode("utf-8", "replace").replace("\r", "").replace("\n", ""))


def startup(binpath, cwd, runs):
    first, substantial = [], []
    for _ in range(runs):
        s = H.PtySession([binpath], 120, 36, cwd=cwd)
        t_sub = None
        deadline = time.monotonic() + 3.0
        while time.monotonic() < deadline:
            data = s.all_bytes()
            if t_sub is None and printable_cells(data) >= 0.3 * 120 * 36:
                with s.lock:
                    acc = b""
                    for c in s.chunks:
                        acc += c.data
                        if printable_cells(acc) >= 0.3 * 120 * 36:
                            t_sub = c.t
                            break
                break
            time.sleep(0.005)
        if s.t_first_byte is not None:
            first.append((s.t_first_byte - s.t_spawn) * 1000)
        if t_sub is not None:
            substantial.append((t_sub - s.t_spawn) * 1000)
        s.send(b"q")
        if s.wait_exit(2) is None:
            s.close(kill=True)
        else:
            s.close(kill=False)
        time.sleep(0.15)
    def summ(v):
        return {"median_ms": round(statistics.median(v), 2) if v else None,
                "min_ms": round(min(v), 2) if v else None, "max_ms": round(max(v), 2) if v else None,
                "n": len(v), "all_ms": [round(x, 2) for x in v]}
    return {"first_byte": summ(first), "first_substantial_frame": summ(substantial)}


def steady(binpath, cwd, warmup=2.0, window=8.0, long_s=0):
    s = H.PtySession([binpath], 120, 36, cwd=cwd)
    pid = s.proc.pid
    time.sleep(warmup)
    if not s.alive():
        return {"error": "process exited during warmup", "exit_code": s.proc.poll()}
    t0 = time.monotonic()
    tk0 = proc_ticks(pid)
    rss = []
    while time.monotonic() - t0 < window:
        rss.append(proc_rss_kib(pid))
        time.sleep(0.25)
    t1 = time.monotonic()
    tk1 = proc_ticks(pid)
    data = s.bytes_since(t0, t1)
    with s.lock:
        bursts = [c.t for c in s.chunks if t0 <= c.t <= t1]
    # frames: group chunks separated by > 30 ms
    frames = 0
    last = None
    for t in bursts:
        if last is None or t - last > 0.03:
            frames += 1
        last = t
    res = {
        "window_s": round(t1 - t0, 3),
        "cpu_pct_one_core": round(100.0 * (tk1 - tk0) / HZ / (t1 - t0), 3),
        "cpu_ticks": tk1 - tk0,
        "rss_kib_mean": round(statistics.mean(rss)), "rss_kib_peak": max(rss), "rss_kib_min": min(rss),
        "vm_hwm_kib": proc_hwm_kib(pid),
        "threads": threads(pid),
        "pty_bytes": len(data), "pty_bytes_per_sec": round(len(data) / (t1 - t0), 1),
        "frames_per_sec": round(frames / (t1 - t0), 2),
        "bytes_per_frame": round(len(data) / frames, 1) if frames else None,
        "steady_seq_counts": H.seq_counts(data),
        "steady_full_clear_present": (data.count(b"\x1b[2J") + len(re.findall(rb"\x1bc", data))) > 0,
        "full_screen_equivalent_bytes": 120 * 36,
    }
    if long_s:
        samples = []
        t_end = time.monotonic() + long_s
        while time.monotonic() < t_end and s.alive():
            samples.append(proc_rss_kib(pid))
            time.sleep(1.0)
        res["long_run"] = {"seconds": long_s, "rss_first_kib": samples[0] if samples else None,
                           "rss_last_kib": samples[-1] if samples else None,
                           "rss_max_kib": max(samples) if samples else None,
                           "alive": s.alive()}
    s.send(b"q")
    if s.wait_exit(2) is None:
        s.close(kill=True)
    else:
        s.close(kill=False)
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=15)
    ap.add_argument("--long", type=int, default=0)
    ap.add_argument("ids", nargs="+")
    a = ap.parse_args()
    targets = json.load(open(os.path.join(ROOT, "scripts", "targets.json")))
    for sid in a.ids:
        cwd = os.path.join(ROOT, "work", sid)
        binpath = os.path.join(cwd, targets[sid]["bin"])
        with open("/proc/loadavg") as fh:
            load_before = fh.read().split()[:3]
        res = {"sid": sid, "binary": targets[sid]["bin"], "binary_size_bytes": os.path.getsize(binpath),
               "host_loadavg_before": load_before, "measured_at": time.strftime("%F %T")}
        res["startup"] = startup(binpath, cwd, a.runs)
        res["steady"] = steady(binpath, cwd, long_s=a.long)
        with open("/proc/loadavg") as fh:
            res["host_loadavg_after"] = fh.read().split()[:3]
        H.dump_json(res, os.path.join(ROOT, "artifacts", sid, "perf.json"))
        st = res["steady"]
        print(f"{sid}: start={res['startup']['first_byte']['median_ms']}ms "
              f"frame={res['startup']['first_substantial_frame']['median_ms']}ms "
              f"cpu={st.get('cpu_pct_one_core')}% rss={st.get('rss_kib_peak')}KiB "
              f"B/s={st.get('pty_bytes_per_sec')} clear={st.get('steady_full_clear_present')} "
              f"size={res['binary_size_bytes']}", flush=True)


if __name__ == "__main__":
    main()
