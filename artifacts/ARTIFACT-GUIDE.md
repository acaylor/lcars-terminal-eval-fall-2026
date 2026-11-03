# Artifact guide (per snapshot `artifacts/SXX/`)

| File | Produced by | Meaning |
| :-- | :-- | :-- |
| `build-summary.json` | `scripts/build_test.sh` | exit code + seconds for each build/test/fmt/lint step (Linux host) |
| `build-release-locked.log` / `build-release.log` | build_test.sh | release build output (Rust: `cargo build --release --locked`; Go: Makefile `make build` or README command) |
| `build-warning-count.txt`, `clippy-warning-count.txt`, `clippy-error-count.txt` | build_test.sh | counts of `^warning` / `^error` lines |
| `test.log` (+ `test-verbose.log` for Go) | build_test.sh | `cargo test --all-targets` / `go test ./...` |
| `fmt-check.log` | build_test.sh | `cargo fmt --check` / `gofmt -l .` |
| `clippy-warnings.log`, `clippy-deny.log` | build_test.sh | `cargo clippy --all-targets` with and without `-D warnings` |
| `vet.log`, `vet-darwin-arm64.log`, `build-darwin-arm64.log` | build_test.sh | `go vet`, and cgo-free `GOOS=darwin GOARCH=arm64` vet/build |
| `clippy-darwin-deny.log` | Linux host, `--target aarch64-apple-darwin` | clippy over the macOS cfg code paths |
| `macos-*.log`, `macos-build-summary.txt` | `scripts/build_test_macos.sh` on macOS 27.2 arm64 host | native macOS release build + tests (+ go vet). (macOS clippy unavailable on that host: toolchain lacked the clippy component — environment issue, not a submission failure.) |
| `static-scan.json` / `.txt` | `scripts/static_scan.py` | regex index of subprocess/unsafe/cgo/TODO/scope hits — **many false positives; verify by reading source** |
| `pty-probe.json` | `scripts/pty_probe.py` (Linux) | PTY runtime: primary/diagnostic captures, controls (`?`, Space, `p`, Tab), lifecycle (q/Esc/Ctrl-C byte/SIGINT/SIGTERM: exit, termios restoration, restore sequences), sizes (80x24, 40x10, 20x5), live SIGWINCH resize, color/ASCII modes, non-TTY |
| `pty-probe-macos.json` | same harness on the macOS host | same probes natively on macOS (termios restoration is `null` there: macOS revokes the slave fd once the session leader exits) |
| `metric-ref-primary-*.json`, `metric-ref-diagnostic-*.json` | harness | live OS reference values snapshotted at capture time (Linux: /proc, statvfs, gethostname, 1 s CPU/net/top-process deltas; macOS: sysctl/vm_stat/df/top/netstat reference output) |
| `nontty-stdout-*.log`, `nontty-stderr-*.log` | harness | `binary </dev/null >out 2>err` with 3 s timeout |
| `perf.json` | `scripts/perf_probe.py` | startup (exec→first PTY byte, and →first substantial frame, 15 runs), steady-state 8 s window after 2 s warmup: CPU % of one core, RSS, PTY bytes/s, clear sequences, frames/s; binary size; 60 s RSS growth check |
| `deliverables.json` | `scripts/setup_snapshots.py` | pre-strip manifest: whether the original submission shipped 120x36 primary/secondary captures and a Captain's Log (captures were stripped from `work/`) |

Fresh captures: `captures-fresh/SXX/*.txt|.ansi|.svg|.png` (Linux) and `captures-fresh/SXX/macos/` (macOS).
Notes on harness heuristics: `metric_checks_*` and `KEY_HINTS` in `pty-probe.json` are regex heuristics (e.g. clocks drawn in large glyph fonts will not match the HH:MM text check). Always confirm against the capture `.txt` and the `metric-ref-*.json` files.

## Lifecycle note: SGR reset
`pty-probe.json → lifecycle.*.restore_seq.sgr_reset` only looks for a reset sequence in bytes emitted **after** the quit stimulus. `sgr-final-state.json` (from `scripts/sgr_final_state.py`) checks whether the **last** SGR sequence in the entire output stream is a reset. If it is, attributes are effectively at default when the terminal is handed back, even without a trailing reset at exit. All nine snapshots end with `\x1b[0m` as their final SGR on both `q` and SIGTERM. Grade the effective state.

## Host facts
- Linux host: x86_64, 24 logical CPUs, kernel 7.0.0-38-generic, Ubuntu 26.04.1, rustc/cargo 1.99.1, go 1.27.3. **The host has no swap configured** (`SwapTotal: 0` in `metric-ref-*.json`), so a "no swap"/"N/A" display is correct.
- macOS host: macOS 27.2 arm64 (Apple Silicon, 10 cores, 24 GiB), rustc 1.99.1, go 1.27.3. Native release builds and tests were run there. Clippy was unavailable on that toolchain, so the macOS cfg paths were linted from Linux with `cargo clippy --all-targets --target aarch64-apple-darwin -- -D warnings` (`clippy-darwin-deny.log`).
- Perf CPU% resolution: /proc ticks at 100 Hz over an 8 s window, so 1 tick = 0.125 %. A reading of 0.0 % means under 1 tick.
- During the PTY probes the harness answered cursor-position, DA1/DA2 and OSC 10/11 queries like a real terminal.
