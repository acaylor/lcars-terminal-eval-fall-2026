#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
BIN=./target/release/lcars-monitor
if [ ! -x "$BIN" ]; then
  echo "missing $BIN — run: cargo build --locked --release" >&2
  exit 1
fi
echo "executable_bytes=$(stat -c%s "$BIN")"
echo "--- startup ---"
/usr/bin/time -f 'time_elapsed_sec=%e' "$BIN" --measure
echo "--- idle sample (4s pty) ---"
python3 - "$BIN" << 'PY'
import os, select, sys, time
binpath = sys.argv[1]
pid, fd = os.forkpty()
if pid == 0:
    os.execv(binpath, [binpath])
start = time.time()
samples = []
while time.time() - start < 4.0:
    r, _, _ = select.select([fd], [], [], 0.2)
    if r:
        try:
            os.read(fd, 1 << 16)
        except OSError:
            break
    try:
        status = open(f"/proc/{pid}/status").read()
        stat = open(f"/proc/{pid}/stat").read()
    except FileNotFoundError:
        break
    rss = hwm = None
    for line in status.splitlines():
        if line.startswith("VmRSS:"):
            rss = int(line.split()[1])
        elif line.startswith("VmHWM:"):
            hwm = int(line.split()[1])
    if rss is not None and hwm is not None:
        samples.append((rss, hwm, stat))
if samples:
    rss_vals = [s[0] for s in samples]
    hwm_vals = [s[1] for s in samples]
    print(f"rss_kib_min={min(rss_vals)}")
    print(f"rss_kib_max={max(rss_vals)}")
    print(f"hwm_kib={max(hwm_vals)}")
    def ticks(raw):
        parts = raw.rsplit(")", 1)[-1].split()
        return int(parts[11]) + int(parts[12])
    dt = ticks(samples[-1][2]) - ticks(samples[0][2])
    elapsed = max(0.001, time.time() - start)
    clk = os.sysconf("SC_CLK_TCK")
    cpu = 100.0 * (dt / clk) / elapsed
    print(f"cpu_percent={cpu:.2f}")
try:
    os.write(fd, b"q")
except OSError:
    pass
os.waitpid(pid, 0)
print("pty_exit=ok")
PY
