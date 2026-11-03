# Methodology — Fall 2026 LCARS Terminal System Monitor (`v3`) Engineering Evaluation

Evaluation date: **2026-11-02**. Scope: the nine submissions `deepseek-v4.1-flash-1017`, `fable-5.1`, `gemini-4`, `gpt-6-astra`, `gpt-6.1-sol`, `grok-4.7`, `kimi-k3.1-pro`, `muse-wonder`, `opus-5.5`. The four other directories in the workspace (`opus-5`, `gemini-3.8`, `sonnet-5.5`, `gpt-6-sol`) were not copied, built or read.

## Hosts and toolchains

| | Linux (all scored runtime + performance measurements) | macOS (native collector cross-check) |
| :-- | :-- | :-- |
| Hardware | AMD Ryzen 9 7900X (12C/24T, up to 5.74 GHz), 93 GiB RAM, **no swap configured** | Apple M4 (10 cores), 24 GiB RAM |
| OS / kernel | Ubuntu 26.04.1 LTS, Linux 7.0.0-38-generic x86_64 | macOS 27.2 (build 26B123), Darwin 27.2.0 arm64 |
| Rust | rustc / cargo 1.99.1, clippy 0.1.99, rustfmt 1.10.0 | rustc / cargo 1.99.1 (no clippy component installed) |
| Go | go 1.27.3 linux/amd64 (`GOTOOLCHAIN=local`) | go 1.27.3 darwin/arm64 |
| Harness | Python 3.14.10, pyte 0.8.2, CairoSVG 2.9.1 | Python 3.14.10, pyte 0.8.2 |

Load conditions: the Linux host was otherwise idle (1-minute load average 0.00–0.40 throughout the performance probe, recorded before and after each binary in `perf.json`). Probes ran one binary at a time.

## Clean-room and anti-bias protocol

1. **Immutable originals.** The original directories were only ever an `rsync` source. Nothing was built, formatted or edited in them.
2. **Anonymized snapshots.** `scripts/setup_snapshots.py` shuffles the nine directory names with `random.Random("lcars-v3-fall-2026-clean-room")` into `work/S01`–`work/S09`. The mapping is written only to `private/submission-map.tsv` and is never printed. Excluded when copying: `.git/`, `target/`, `bin/`, `cost.txt`, `prompt-v3.md`, `captures/`, `artifacts/`, `docs/captures/`, and any ELF or Mach-O file (none remained after the directory excludes). A grep for model or vendor names across every snapshot's `.md/.rs/.go/.toml/.py/.sh` files found **no identity leakage**.
3. **Pre-strip deliverables manifest.** Before stripping captures, `private/deliverables-manifest.json` (keyed by SXX) recorded whether each original shipped 120x36 primary and secondary captures and a Captain's Log. All nine shipped both captures. Eight have a dedicated Captain's Log file; one (S05) has it only as a `## Captain's log` section in its README.
4. **Blind grading.** Each SXX was graded by an isolated subagent. It could read only `prompt-v3.md`, `RUBRIC.md` (§3.4 and §4 of the brief, with the human visual-score table removed), `SCORECARD-TEMPLATE.md`, `artifacts/ARTIFACT-GUIDE.md`, and its own `work/SXX`, `artifacts/SXX` and `captures-fresh/SXX`. It was forbidden to read `private/`, other snapshots, or anything outside `evaluation/`. No source was repaired.
5. **Blind calibration.** After all nine were locked, and still blind, the orchestrator compared the scorecards item by item. Two arithmetic errors were corrected, and three macOS-collector scores were brought onto one severity ladder. Each change is an addendum inside the affected scorecard, and the full record is in `results/CALIBRATION.md`.
6. **Lock, then unblind.** All nine `results/SXX.md` scorecards (plus the calibration) were written, marked `LOCKED` and made read-only before the mapping and the visual scores were opened.

## Probes (scripts in `evaluation/scripts/`)

| Script | What it does |
| :-- | :-- |
| `setup_snapshots.py` | Deterministic shuffle, rsync with excludes, deliverables manifest, binary sweep |
| `build_test.sh` | Rust: `cargo build --release --locked` (with an unlocked fallback that was never needed), `cargo test --all-targets`, `cargo fmt --check`, `cargo clippy --all-targets` (warning count) and `-- -D warnings`. Go: Makefile `make build` or the README release command, `go build ./...`, `go test ./...`, `gofmt -l .`, `go vet ./...`, cgo-free `GOOS=darwin GOARCH=arm64` vet and build |
| `build_test_macos.sh` | Native macOS release build and tests (plus `go vet`) |
| `clippy_darwin.sh` | `cargo clippy --all-targets --target aarch64-apple-darwin -- -D warnings` from Linux, linting the macOS `cfg` code paths. This replaces macOS-native clippy, which was unavailable on that host's toolchain |
| `static_scan.py` | Regex index (subprocess, unsafe, cgo, TODO, scope-creep markers, LOC, test counts, dependency trees). Hits were confirmed by graders reading the source; every subprocess hit was a false positive |
| `lcars_harness.py` | Opens a fresh PTY for each run with the child as session leader and the PTY as its controlling tty. Mirrors output into a pyte screen. Answers CPR, DA1/DA2 and OSC 10/11 queries like a real terminal. Renders captures to `.txt`, replayable `.ansi`, `.svg` and `.png`. Takes live OS reference snapshots |
| `pty_probe.py` | At 120x36 with truecolor: primary capture at 2.5 s, plus a live `/proc`/`statvfs`/`gethostname` reference with 1 s CPU, network and top-process deltas. Then `p` → diagnostic capture, `p` back, `?` open/close, Space pause (2.6 s stability) and resume, Tab ×4, `q`. Lifecycle via `q`, `Esc`, `Ctrl-C` byte, OS SIGINT and SIGTERM, recording slave termios before and after exit and the alt-screen exit, cursor-show and SGR-reset sequences after the stimulus. Sizes 80x24 (+ diagnostic), 40x10 and 20x5. Live resize 120x36 → 80x24 → 30x8 → 120x36 (TIOCSWINSZ + explicit SIGWINCH). Color modes 256 (no COLORTERM), 16 (`TERM=xterm`), `NO_COLOR=1`, the documented mono switch, the documented ASCII switch, `LC_ALL=C` and `TERM=dumb`. Non-TTY run `</dev/null >out 2>err` with a 3 s timeout |
| `sgr_final_state.py` | Checks whether the *last* SGR sequence in the full stream is a reset. Two snapshots emit no reset after the quit stimulus but have already reset at the end of their last frame |
| `perf_probe.py` | Startup: exec → first PTY byte, and → first substantial frame (≥30% of a 120x36 screen of printable cells), median of 15. Steady state: 2 s warmup, then an 8 s window: CPU % of one core from `/proc/<pid>/stat` ticks (100 Hz, so 0.125% resolution), VmRSS (sampled every 250 ms), PTY bytes/s, write bursts/s, and counts of `\x1b[2J` and RIS. Plus a 60 s RSS-growth run and the release binary size |

## Harness caveats and decisions
- **macOS termios.** macOS revokes the PTY slave once the session leader exits, so termios-after-exit is recorded as `null` on macOS. Termios restoration is graded from the Linux run, where it is exact.
- **macOS child-gone writes.** On Darwin, writing to the master after the child has exited returns EIO. The harness tolerates this. The macOS `color_modes` step was re-run with `--steps color_modes` for the five snapshots that refuse `TERM=dumb`, and the results were merged.
- **Heuristic checks.** The regex `metric_checks` and `KEY_HINTS` in `pty-probe.json` are heuristics. Graders were told to confirm by eye from the captures and reference JSON, since large glyph-font clocks, for example, defeat the HH:MM text match.
- **"`\x1b[c`" in the brief.** This is DA1, not a clear. Graders looked for `\x1b[2J` and RIS (`\x1bc`) instead. No snapshot emitted either during the steady-state window.
- **PNG/SVG colour mapping.** The capture renderer missed pyte's `bfightmagenta` colour name (a pyte typo for bright magenta), so bright-magenta backgrounds are missing from the `color16-*` images. The raw streams and the SGR statistics in `pty-probe.json` are correct, and graders checked against those. No score depends on the PNGs.
- **macOS clippy.** The macOS host's mise-managed toolchain lacked `cargo-clippy`. Darwin-path linting was done from Linux against the `aarch64-apple-darwin` target, which is check-only and needs no SDK. Go Darwin vetting was native (`go vet` on macOS) and cross-compiled with cgo disabled.

## Scoring
Engineering is out of 70 (seven categories), with honesty deductions of 0 to −10 and the human visual score (1–10 × 3 = 30) added only after locking. The build gate was not triggered. All nine compiled on Linux with zero warnings and built natively on macOS. One snapshot's macOS build emitted 7 deprecation warnings for libc Mach symbols, which fail `clippy -D warnings` for the darwin target.
