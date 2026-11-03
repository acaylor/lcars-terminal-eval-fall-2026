#!/usr/bin/env bash
# Lint the macOS cfg code paths from Linux: cargo clippy for aarch64-apple-darwin (check only, no link).
EVAL="$(cd "$(dirname "$0")/.." && pwd)"; export CARGO_TERM_COLOR=never
for id in "$@"; do
  cd "$EVAL/work/$id" || continue; A="$EVAL/artifacts/$id"
  { echo "\$ cargo clippy --all-targets --target aarch64-apple-darwin -- -D warnings"
    cargo clippy --all-targets --target aarch64-apple-darwin -- -D warnings; } >"$A/clippy-darwin-deny.log" 2>&1
  echo "$id rc=$? errors=$(grep -c '^error' "$A/clippy-darwin-deny.log")"
done
