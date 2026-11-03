#!/usr/bin/env bash
# Build, test, format and static-analysis probe for one or more clean snapshots.
# Usage: build_test.sh [S01 S02 ...]   (default: all S01..S09)
# Logs:  evaluation/artifacts/SXX/{build,test,fmt,lint,...}.log and build-summary.json
set -u
EVAL="$(cd "$(dirname "$0")/.." && pwd)"
export GOTOOLCHAIN=local CARGO_TERM_COLOR=never
ids=("$@"); [ ${#ids[@]} -eq 0 ] && ids=(S01 S02 S03 S04 S05 S06 S07 S08 S09)

run() { # run <name> <cmd...> ; logs to $A/<name>.log, records exit code + seconds
  local name="$1"; shift
  local t0=$(date +%s.%N)
  { echo "\$ $*"; "$@"; } >"$A/$name.log" 2>&1
  local rc=$?
  local t1=$(date +%s.%N)
  printf '  "%s": {"rc": %d, "seconds": %.1f},\n' "$name" "$rc" "$(echo "$t1 - $t0" | bc)" >>"$A/.summary.part"
  return $rc
}

for id in "${ids[@]}"; do
  W="$EVAL/work/$id"; A="$EVAL/artifacts/$id"; mkdir -p "$A"; : >"$A/.summary.part"
  cd "$W" || continue
  echo "== $id"
  if [ -f Cargo.toml ]; then
    echo rust >"$A/language.txt"
    if ! run build-release-locked cargo build --release --locked; then
      run build-release-unlocked cargo build --release   # lockfile drift fallback
    fi
    grep -c '^warning' "$A/build-release-locked.log" >"$A/build-warning-count.txt" || true
    run test cargo test --all-targets
    run fmt-check cargo fmt --check
    run clippy-warnings cargo clippy --all-targets          # count warnings without -D
    run clippy-deny cargo clippy --all-targets -- -D warnings
    grep -c '^warning' "$A/clippy-warnings.log" >"$A/clippy-warning-count.txt" || true
    grep -c '^error' "$A/clippy-deny.log" >"$A/clippy-error-count.txt" || true
    ls -la target/release/ >"$A/release-dir.txt" 2>&1
  elif [ -f go.mod ]; then
    echo go >"$A/language.txt"
    if [ -f Makefile ]; then
      run build-release make build
    else  # README-documented release command
      run build-release go build -trimpath -ldflags='-s -w' -o bin/lcars ./cmd/lcars
    fi
    run build-all go build ./...
    run test go test ./...
    run fmt-check sh -c 'out=$(gofmt -l .); echo "$out"; test -z "$out"'
    run vet go vet ./...
    run vet-darwin-arm64 env GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go vet ./...
    run build-darwin-arm64 env GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -o "$A/lcars-darwin-arm64" ./cmd/lcars
    ls -la bin/ >"$A/release-dir.txt" 2>&1
  fi
  { echo "{"; sed '$ s/,$//' "$A/.summary.part"; echo "}"; } >"$A/build-summary.json"; rm -f "$A/.summary.part"
  cat "$A/build-summary.json"
done
