# lcars-monitor

LCARS-inspired terminal engineering display for macOS and Linux. The primary view is a framed command composition, not a boxed system monitor.

## Language decision

Rust. A native binary starts in well under a millisecond, has no garbage collector, and can read Linux `/proc` and macOS Mach/libproc through the same source tree. Rendering is a cell buffer written with differential ANSI, so the terminal is not cleared on each tick. One dependency, `libc` 0.2.189, supplies termios, signals, `statvfs`, and the macOS system calls. No TUI framework: the frame is custom, and the lifecycle code is small enough to restore the terminal on panic and on `SIGINT`/`SIGTERM`.

## Architecture

- `src/metrics` collects samples. Rate math and `/proc` parsers are pure and tested without a live machine. `Collector` hides `linux` and `macos` modules behind one `sample(include_processes)` call. A failed reader leaves that field unavailable and does not fail the rest of the sample.
- `src/render` paints a `Buffer` of cells. Layout, fonts, and views do not touch the terminal. `diff_rows` emits only changed rows.
- `src/term` owns raw mode, the alternate screen, cursor hiding, `SIGWINCH`, and restoration.
- `src/app` is the loop: sample on the refresh interval, paint, poll keys. It is not the collector and not the painter.

Metric refresh is 1 second. Rates use the measured gap between samples, not the nominal interval.

## Controls

| Key | Action |
| --- | --- |
| `q` or `Esc` | Quit |
| `Ctrl-C` | Quit (`SIGINT`) |
| `?` | Help overlay |
| Space | Pause or resume metric updates |
| `p` | Open or close the process diagnostic view |
| Tab | Cycle focus across the telemetry rails or process rows |

The bottom rail shows `Q ? P TAB SPC`. The process list is not on the primary view.

`LCARS_ASCII=1` forces the ASCII fallback. `LCARS_MONO=1` or `NO_COLOR` forces monochrome. True color is used when `COLORTERM` contains `truecolor` or `24bit`; otherwise the same layout uses 16-color ANSI. Loss of true color does not change geometry.

## macOS and Linux

Linux reads `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/uptime`, `/proc/net/dev`, and `/proc/[pid]/stat`. Disk is `statvfs("/")`. Nothing is shelled out.

macOS uses `host_statistics64`, `host_processor_info`, `getloadavg`, `sysctl` (`hw.memsize`, `kern.boottime`, `vm.swapusage`), `NET_RT_IFLIST2` for 64-bit interface counters, and `proc_pidinfo` / `proc_listallpids`. That path typechecks for `x86_64-apple-darwin` and `aarch64-apple-darwin`. It was not executed on a Mac in this workspace.

A counter that decreases is treated as a reset. That interval is skipped instead of inventing a wrap width. A new interface is baselined and not counted as a delta. A vanished interface is dropped. The first CPU and network sample is unavailable until a second sample exists.

## Release build

```sh
cargo build --locked --release
./target/release/lcars-monitor
```

`Cargo.lock` pins `libc` 0.2.189. `--locked` keeps the build reproducible. The first fetch needs a network; later builds can use `cargo build --locked --release --offline` when the crate is cached. The release profile uses LTO, `opt-level = 3`, stripped symbols, and `panic = abort`.

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo check --locked --target x86_64-apple-darwin --target aarch64-apple-darwin
./scripts/measure.sh
./target/release/lcars-monitor --dump primary --width 120 --height 36 --shape --demo
./target/release/lcars-monitor --dump secondary --width 120 --height 36 --shape --demo
```

`--measure` initializes the collector, takes one sample, prints `startup_us`, and exits. That is the startup measurement, not a monitoring mode.

## Captures

`captures/primary-120x36.txt` and `captures/secondary-120x36.txt` are silhouette maps of the 120×36 renderer (`O` orange, `A` amber, `S` salmon, `L` lavender, `B` blue, `P` peach, `W` scanner, `#` glyph on black, space empty). The `.ansi` files are the same frames with color. They use a fixed demo sample so every required readout is populated; live sampling is covered by `tests/layout.rs`.

## Known limitations

- macOS collection is implemented and typechecked, not run on Apple hardware here.
- CPU, network, and process CPU need two samples. The first interval shows an unavailable mark, not zero.
- Network rate is the sum of non-loopback interfaces, not a per-interface breakdown.
- macOS memory available is `free + inactive + speculative` pages, capped by `hw.memsize`. It is an accounting estimate, not an Activity Monitor clone.
- `SIGKILL` cannot restore the terminal. Normal exit, `Ctrl-C`, and `SIGTERM` can.
- True color is detected from the environment, not by a probe that could hang.
- East Asian wide characters are treated as width 2. Obscure widths are approximate.
- At 80×24 the command display drops the sparkline, separate RX/TX rails, and disk pill rather than squeezing them in. At very small sizes the frame collapses to a single rail and one status line.
- No GPU, thermal, or battery metrics. Unsupported fields stay unavailable.
