#!/usr/bin/env bash
# Native macOS cross-check: release build, tests, and lint of the Darwin code paths.
# Run on the macOS host from the evaluation root copy (~/lcars-eval).
# Logs: artifacts/SXX/macos-*.log and macos-build-summary.txt
set -u
EVAL="$(cd "$(dirname "$0")/.." && pwd)"
export GOTOOLCHAIN=local CARGO_TERM_COLOR=never
ids=("$@"); [ ${#ids[@]} -eq 0 ] && ids=(S01 S02 S03 S04 S05 S06 S07 S08 S09)
for id in "${ids[@]}"; do
  W="$EVAL/work/$id"; A="$EVAL/artifacts/$id"; mkdir -p "$A"; S="$A/macos-build-summary.txt"; : >"$S"
  cd "$W" || continue
  r() { local n="$1"; shift; { echo "\$ $*"; "$@"; } >"$A/macos-$n.log" 2>&1; echo "$n rc=$?" >>"$S"; }
  if [ -f Cargo.toml ]; then
    r build-release cargo build --release --locked
    r test cargo test --all-targets
    r clippy-deny cargo clippy --all-targets -- -D warnings
  else
    if [ -f Makefile ]; then r build-release make build
    else r build-release go build -trimpath -ldflags='-s -w' -o bin/lcars ./cmd/lcars; fi
    r test go test ./...
    r vet go vet ./...
  fi
  echo "== $id"; cat "$S"
done
