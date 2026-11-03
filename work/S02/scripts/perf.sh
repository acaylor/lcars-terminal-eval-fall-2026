#!/usr/bin/env sh
# Startup time, idle CPU, peak memory, terminal output rate and binary size.
# Needs only python3 (stdlib pty) — no terminal emulator, no network.
set -eu
cd "$(dirname "$0")/.."
BIN=./target/release/lcarsmon
[ -x "$BIN" ] || cargo build --release
SECS=${1:-30}
if stat -c %s "$BIN" >/dev/null 2>&1; then SIZE=$(stat -c %s "$BIN"); else SIZE=$(stat -f %z "$BIN"); fi
echo "binary size: $SIZE bytes ($(( SIZE / 1024 )) KiB, stripped release)"
python3 scripts/pty_harness.py startup "$BIN" 20
python3 scripts/pty_harness.py resources "$BIN" "$SECS" command
python3 scripts/pty_harness.py resources "$BIN" "$SECS" diagnostic
