# LCARS Monitor

A native, LCARS-inspired engineering workstation display for macOS and Linux. The default command view combines an asymmetric elbow frame, oversized half-block clock, animated status, open telemetry, and a Braille CPU waveform. Per-core and process diagnostics live on a separate view.

## Engineering decision and architecture

**Rust** was selected for fast native startup, low and bounded runtime overhead, reliable terminal I/O, and macOS/Linux support without a language runtime. The release profile enables LTO, one codegen unit, and symbol stripping.

- `src/metrics.rs`: OS-backed sampling behind the `Collector` interface. `sysinfo` uses Linux `/proc`/sysfs and macOS host APIs/sysctls; the application never shells out for refreshes.
- `src/model.rs`: terminal-independent snapshots, elapsed-time network rates, counter-reset handling, and bounded history.
- `src/render.rs`: responsive primary/diagnostic compositions, compact 80×24 layout, minimal tiny-terminal layout, help, Unicode and ASCII paths.
- `src/buffer.rs`: width-aware styled cells, true-color/ANSI/monochrome palettes, and synchronized changed-cell diffs. Regular frames do not clear the terminal.
- `src/terminal.rs` and `src/main.rs`: poll-driven input, one-second sampling, resize handling, signals, and RAII terminal restoration.

### Direct dependencies

- **sysinfo 0.39**: consistent real CPU, memory, swap, disk, network, uptime, load, and process metrics on macOS and Linux.
- **crossterm 0.29**: raw input, events, alternate screen, cursor control, synchronized updates, and ANSI colors.
- **signal-hook 0.3**: async-signal-safe termination flags for SIGINT, SIGTERM, and SIGHUP.
- **unicode-width 0.2**: correct clipping and alignment of wide non-ASCII names.
- **libc 0.2**: thread-safe `localtime_r` for local date/time without formatting subprocesses.

No runtime network connection, elevated privileges, configuration file, or third-party service is required.

## Build and run

Requires Rust **1.95 or newer**. Dependency resolution needs a network only before the project is built; the installed application is fully offline.

```sh
cargo build --release --locked
./target/release/lcars-monitor
```

The executable is `target/release/lcars-monitor` (`.exe` is not used on macOS/Linux). Build on each target OS; one cross-platform binary is not implied.

Controls are always summarized in the footer:

| Key | Action |
| --- | --- |
| `q`, `Esc`, Ctrl-C | Quit and restore the terminal |
| Space | Pause/resume sampling |
| `p` | Toggle process diagnostics |
| Tab | Cycle command views |
| `?` | Open/close command reference |

`--mono` or `NO_COLOR` selects monochrome. Without true color (`COLORTERM=truecolor`/`24bit` or a direct-color `TERM`), a small ANSI palette preserves the layout. `--ascii` replaces all non-ASCII glyphs, including dynamic names. `--snapshot primary` and `--snapshot secondary` render a real sampled 120×36 frame to redirected output; interactive mode otherwise rejects unsuitable terminals with an actionable error.

## Captures and visual verification

- [Primary 120×36 text](captures/primary-120x36.txt) · [color SVG](captures/primary-120x36.svg)
- [Secondary 120×36 text](captures/secondary-120x36.txt) · [color SVG](captures/secondary-120x36.svg)

Regenerate both from live metrics:

```sh
cargo run --release --locked --example capture
```

The captures use thirteen actual timed samples per view. The primary acceptance checks pass: asymmetric silhouette; one dominant clock; at least 20% blank background; frame-integrated metrics rather than dashboard boxes; text-independent colored rail structure; no resemblance to a process table; and no process list until diagnostics are selected. The negative-space, focal-glyph, view-separation, and rail-structure properties also have automated tests.

## Verification and performance

Exact commands used:

```sh
cargo fmt --all -- --check
cargo build --release --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
python3 scripts/smoke.py target/release/lcars-monitor
python3 scripts/measure.py target/release/lcars-monitor
```

Results on Linux x86-64 with Rust 1.99.1:

- Formatting, release build, and Clippy with warnings denied: **passed**.
- **14 tests passed**, covering elapsed-time rates, resets/zero/long intervals, bounded history, formatting, Unicode width, tiny buffers, diff rendering, required metrics, responsive layouts, negative space, and ASCII output.
- Pseudo-terminal smoke test: controls, 120×36 → 80×24 resize, `q`, Ctrl-C, SIGTERM, cursor/alternate-screen restoration, and termios restoration **passed**.
- Interactive first frame: **118.2 ms**.
- Five-second CPU measurement including startup: **2.73% of one core**.
- Peak RSS: **16.98 MiB**; stripped executable: **720,792 bytes**.

`scripts/measure.py` uses a pseudo-terminal and Python's standard `resource` module. It works on macOS and Linux; measurements are host-specific and should be rerun for release comparisons.

## Known limitations

- The build and runtime paths use cross-platform APIs, but only Linux was executed in this environment; macOS still needs an on-host smoke test.
- Network rates intentionally show `ACQUIRING BASELINE` until two valid counter samples exist. Counter resets reacquire rather than emitting negative rates.
- Process CPU is the OS-reported interval value and may exceed 100% for multithreaded processes. Very large core counts use compact numeric cells on smaller terminals.
- Unicode appearance depends on the terminal font and locale. Monospace fallback is ASCII-safe; wide grapheme clusters are clipped by terminal column width rather than grapheme semantics.
- Only the root filesystem is shown, by design. Missing swap, inaccessible disks, and unsupported metrics use unavailable states instead of fabricated values.

See [CAPTAINS_LOG.md](CAPTAINS_LOG.md) for the final verification log.
