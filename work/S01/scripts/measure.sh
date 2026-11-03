#!/usr/bin/env bash
# Performance measurement for lcars. Run from the project root after
# `cargo build --release`. Needs tmux (startup and idle-CPU numbers are taken
# in a real pseudo-terminal) and /usr/bin/time on Linux or macOS.
set -euo pipefail
BIN=${BIN:-target/release/lcars}
SECS=${SECS:-30}
[ -x "$BIN" ] || { echo "build first: cargo build --release" >&2; exit 1; }

echo "== executable size"
ls -l "$BIN" | awk '{printf "%s bytes (%.1f MiB)\n", $5, $5/1048576}'

echo "== startup: process start -> first frame flushed (real pty, 5 runs)"
for i in 1 2 3 4 5; do
  tmux -f /dev/null new-session -d -s lcars_m -x 120 -y 36 \
    "TERM=xterm-256color COLORTERM=truecolor $BIN --exit-after 0 2> /tmp/lcars_startup.$$; exec sleep 5"
  sleep 1
  cat /tmp/lcars_startup.$$
  tmux kill-session -t lcars_m
done
rm -f /tmp/lcars_startup.$$

echo "== headless frame (fork+exec, two samples 300 ms apart, render, exit)"
if command -v hyperfine >/dev/null; then
  hyperfine -N --warmup 3 "$BIN --snapshot 120x36"
else
  s=$(date +%s%N); "$BIN" --snapshot 120x36 > /dev/null; e=$(date +%s%N)
  echo "wall $(( (e - s) / 1000000 )) ms (includes the 300 ms sampling sleep)"
fi

echo "== idle CPU and memory over ${SECS}s at 120x36 (1 s sampling, 4 fps animation)"
if [ "$(uname)" = "Darwin" ]; then TIMEFMT="-l"; else TIMEFMT="-v"; fi
tmux -f /dev/null new-session -d -s lcars_m -x 120 -y 36 \
  "TERM=xterm-256color COLORTERM=truecolor /usr/bin/time $TIMEFMT $BIN --exit-after $((SECS * 1000)) 2> /tmp/lcars_time.$$; exec sleep 5"
sleep $((SECS + 2))
tmux kill-session -t lcars_m || true
grep -E "first frame|User time|System time|Maximum resident|Percent of CPU|real|user|sys|maximum resident" /tmp/lcars_time.$$ || cat /tmp/lcars_time.$$
rm -f /tmp/lcars_time.$$

echo "== bytes written per second while idle (diff renderer output), 10 s"
tmux -f /dev/null new-session -d -s lcars_m -x 120 -y 36 \
  "TERM=xterm-256color COLORTERM=truecolor strace -f -e trace=write -o /tmp/lcars_strace.$$ $BIN --exit-after 10000 2>/dev/null; exec sleep 5" 2>/dev/null || true
sleep 12
tmux kill-session -t lcars_m 2>/dev/null || true
if [ -s /tmp/lcars_strace.$$ ]; then
  awk -F'= ' '/write\(1,/ {s+=$NF; n++} END {printf "%d writes, %d bytes total, %.0f bytes/s\n", n, s, s/10}' /tmp/lcars_strace.$$
else
  echo "strace not available; skipped"
fi
rm -f /tmp/lcars_strace.$$
