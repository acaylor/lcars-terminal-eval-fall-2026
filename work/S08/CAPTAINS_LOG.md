# Captain's Log — LCARS 47

**Date:** 2026-10-27  
**Mission:** Build an original LCARS-inspired terminal system monitor for Linux
and macOS, prioritizing the primary visual composition, correct lifecycle and
rendering, accurate metrics, then diagnostics and documentation.

## Entry 1 — Design and build

Built a single-threaded Rust application with a strict separation between
sampling and presentation:

- `Collector` + `SysinfoCollector` gather CPU, memory, swap, load, uptime,
  processes, and network counters; `statvfs` handles root disk usage.
- `RateTracker` converts cumulative network counters into rates using measured
  elapsed time, with per-interface state, reset detection, and bad-interval
  rejection.
- A 5x9 Unicode block clock, asymmetric header rail, left command spine, blue
  elbow CPU field, load/trace field, memory rail, and network/root tags form the
  primary composition.
- A separate diagnostics manifest shows OS/architecture, every per-core gauge,
  swap, process count, and the five busiest processes.
- A dedicated help panel, 80x24 compact layout, and text-only tiny layout cover
  the remaining sizes.
- `Presenter` diffs a cell buffer and writes only changed cells.

No metric is fabricated. Missing or unsupported data is shown as `N/A`, `SYNC`,
`NO SWAP`, or a dashed unavailable rail.

## Entry 2 — Verification

Verified on Linux 7.0.0-38-generic x86_64 (24 logical cores) with Rust 1.99.1:

- `cargo build --release --locked`: success
- `cargo test --locked`: 38 passed, 0 failed
- `cargo clippy --all-targets --locked -- -D warnings`: clean
- `cargo fmt --all -- --check`: clean
- Interactive PTY checks: `q`, `Ctrl-C`, `Esc`, `?`, `Space`, `p`, `Tab`,
  resize, `SIGTERM`; alternate screen/cursor/raw mode restored every time.
- Redirected stdout and `TERM=dumb` produce actionable errors, not corrupted
  output.
- ASCII fallback is entirely ASCII; the 120x36 captures have exactly 36 rows of 120 cells.

Live performance:

- Startup to first frame: **38.4–42.6 ms**, median **39.1 ms** (7 PTY runs)
- Idle CPU: **0.62% of one core** over 8 seconds
- RSS: **5,176 KiB**
- Idle terminal output: **~5.7 KB/s**
- Release binary: **823,688 bytes (804 KiB)**

## Entry 3 — Visual verification

Captured the live application at 120x36 after ten real samples and assessed the
visual acceptance questions:

1. Asymmetric silhouette: **yes**
2. One dominant focal element: **yes**, the 71-column clock
3. At least 20% negative space: **yes**
4. Metrics integrated, not interchangeable boxes: **yes**
5. Recognizable with text removed: **yes**
6. Materially different from top/htop/btop: **yes**
7. Process list absent until `p`: **yes**

At 80x24 the command display deliberately simplifies rather than compressing
everything. At larger sizes the full composition returns, with a wider clock and
an auxiliary systems manifest.

## Entry 4 — Limitations and handover

The Linux path is fully exercised. The macOS path uses the same cross-platform
crates plus POSIX `statvfs`, `localtime_r`, and signal calls, but no macOS
hardware was available for execution testing; this is the principal unverified
item. Network rates aggregate all interfaces, disk usage is root-only, and
diagnostics at extremely small sizes can truncate the per-core matrix with an
explicit note.

The project builds with `cargo build --release --locked`; captures regenerate
with `cargo run --release --bin lcars-capture`. The primary and diagnostic
screenshots, help panel, and 80x24 variants are in `captures/`.

End of log. Maximum warp.
