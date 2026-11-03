# LCARS 47 // Terminal Systems Monitor

An original, LCARS-inspired terminal command display for monitoring a Linux or
macOS workstation. It is a real system monitor, not a reskin of `top`, `htop`,
or `btop`: the primary screen is built around a dominant amber clock, a thick
left command spine, an asymmetric right elbow field, integrated telemetry rails,
and deliberate negative space.

![Primary command view, 120x36](captures/primary-120x36.png)

![Process diagnostics, 120x36](captures/diagnostics-120x36.png)

## Engineering decision

**Language: Rust.**

Rust gives this application what the brief prioritizes: a native executable
with no garbage collector or language runtime, predictable sub-100 ms startup,
low idle CPU, and direct control over terminal output. The same source builds on
current Linux and macOS with no per-platform UI code. The release binary is
about 800 KiB and starts in roughly 40 ms on the test machine.

The implementation deliberately avoids an async runtime. A single thread polls
input, samples metrics on a one-second schedule, animates a few status marks at
10 fps, and diffs a cell buffer before writing to the terminal. There is no
network access at runtime and no external process is spawned for metrics.

### Third-party libraries

| Crate | Version | Why |
| --- | --- | --- |
| `crossterm` | 0.29.0 | Cross-platform raw mode, alternate screen, resize, cursor, and key events on Linux/macOS. |
| `sysinfo` | 0.39.6 (`system`, `network`) | Battle-tested OS-specific CPU, memory, swap, load, uptime, process, and network collectors for both platforms. It uses `/proc` on Linux and native APIs on macOS. Avoiding a hand-written macOS Mach/sysctl collector reduces portability risk. |
| `libc` | 0.2.x | `statvfs` for the root filesystem, `localtime_r` for the wall clock, and POSIX signal handlers. |
| `unicode-width` | 0.2.3 | Safe truncation of long and non-ASCII process names without corrupting the fixed grid. |

The only `unsafe` code is the small POSIX boundary in `collect.rs` and
`term.rs` (statvfs, localtime_r, signal registration). All rendering and
sampling logic is safe Rust.

## Architecture

```
src/
  collect.rs   Collector trait + sysinfo/statvfs collector; OS data isolated here
  model.rs     Metrics, RateTracker, formatting, name sanitization, rate tests
  clock.rs     localtime_r wall clock, date labels
  layout.rs    Density selection and explicit rectangles for 80x24 through large screens
  glyphs.rs    5x9 clock font, pills, elbow capsule, thin/segmented meters, sparklines, braille
  buffer.rs    Cell grid, background-preserving text, ANSI diff presenter, tests
  theme.rs     Semantic palette, truecolor/256/16/mono mappings, detection
  views.rs     Primary (full/compact/tiny), diagnostics (full/compact/tiny), help
  app.rs       CLI, state machine, sampling schedule, histories, snapshot mode
  term.rs      Terminal lifecycle, panic/signal restore, key mapping
  main.rs      Thin entry point
  bin/capture.rs  Development capture generator (HTML/TXT/ANSI)
scripts/
  measure-startup.py  PTY startup measurement
  measure-idle.py     Linux idle CPU/RSS/output measurement
```

Data flow:

```
Collector::sample(elapsed)
        -> Metrics
        -> views::render_frame(...) -> Buffer<Cell>
        -> Presenter::present(...)  -> minimal ANSI cursor/style writes
        -> terminal
```

- Metric collection never imports presentation code.
- `Collector` is the stable boundary; `SysinfoCollector` is the only host
  implementation.
- Rendering accepts a `UiState` and a size, so every view can be tested without
  a terminal.
- `RateTracker` uses measured elapsed time from `Instant`, not an assumed
  refresh interval. It tracks cumulative counters per interface, skips counter
  resets, handles interfaces appearing/disappearing, and rejects intervals
  shorter than 50 ms or longer than 30 s.
- Histories are capped at 120 samples, keeping memory bounded.
- The presenter keeps a previous cell buffer and writes only changed cells.
  Full clears happen only on resize or explicit view transitions.
- A failed sample is caught with `catch_unwind`; the last good display is kept
  and a `SENSOR FAULT` status is shown instead of crashing.

## Controls

| Key | Action |
| --- | --- |
| `q` | Quit |
| `Esc` | Close help, otherwise quit |
| `?` | Toggle the command reference |
| `Space` | Hold / resume sampling (wall clock and animation freeze) |
| `p` | Open / close the process diagnostics |
| `Tab` | Cycle command and diagnostic views |
| `Ctrl-C` | Restore the terminal and exit |

The footer always lists the primary controls, and `?` shows the full reference
with a telemetry color legend.

## Views

- **Primary / command** (`q` view): local time with seconds as a 5x9 block
  clock, date, hostname, uptime, overall CPU, a large 1-minute load readout with 5/15-minute companions, memory
  used/available/total, root filesystem, and aggregate RX/TX rates. At `120x36`
  the clock is scaled 2x horizontally; at very large terminals it grows to 3x
  and an auxiliary systems manifest appears.
- **Diagnostics** (`p` or `Tab`): OS and architecture, all per-core CPU bars,
  swap, process count, load averages, and the five highest-CPU processes with
  PID, name, CPU, and memory. Process data never appears on the primary screen.
- **Help** (`?`): full control reference and palette legend on a dedicated
  panel.
- **80x24**: a deliberately simplified command layout with all required primary
  metrics, a 1x clock, a compact CPU capsule, and abbreviated rails.
- **Tiny terminals**: a text-only degradation with no panics or out-of-bounds
  drawing.

### Color and glyph modes

- 24-bit color is detected from `COLORTERM`/`TERM` and used when available.
- `--color 256`, `--color 16`, and `--color mono` provide explicit reduced
  modes. `NO_COLOR` selects the monochrome treatment.
- `--ascii` forces an ASCII-safe fallback (all glyphs are ASCII; the layout and
  color fields remain). `--unicode` forces Unicode.
- Without a UTF-8 locale the application starts in ASCII mode.
- Loss of true color changes fidelity only; layout and information are
  unaffected.

## Platform implementation

- **Linux**: `sysinfo` reads `/proc` for CPU, memory, swap, load, uptime,
  processes, and network counters. `statvfs("/")` provides root filesystem
  usage. Wall clock via libc `localtime_r`.
- **macOS**: `sysinfo` uses native Mach/sysctl APIs for the same metrics, and
  the same `statvfs`, `localtime_r`, and POSIX signal code applies. No `/proc`
  dependency exists in the application.
- Truecolor, cursor, alternate screen, raw mode, resize, and key input are
  handled by `crossterm` on both platforms.
- The terminal title, cursor, line wrapping, raw mode, and alternate screen are
  restored on quit, `Ctrl-C`, `SIGINT`, `SIGTERM`, `SIGHUP`, and panics.
- Redirected output and `TERM=dumb` are rejected with actionable messages
  instead of printing escape soup.

## Build

Prerequisites:

- Rust 1.95 or newer (tested with `rustc 1.99.1`)
- On macOS: Xcode Command Line Tools (`xcode-select --install`)
- No network connection is needed after the binary is built.

```sh
cargo build --release --locked
./target/release/lcars-mon
```

The dev capture generator:

```sh
cargo run --release --bin lcars-capture
```

That waits for ten real one-second samples, then writes the primary, diagnostics,
80x24, help, and ASCII captures into `captures/`. The HTML captures are the
canonical color captures; the PNGs in this repository were rasterized from those
HTML files with headless Chromium. Values in the captures are live readings from
the test machine and will differ on rerun.

A headless single frame (useful for CI or a quick look) is available through the
main binary:

```sh
./target/release/lcars-mon --snapshot --plain
./target/release/lcars-mon --snapshot=diagnostics --plain --size 120x36
./target/release/lcars-mon --snapshot --ascii --plain
```

## Test, lint, and format

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Results on the verification machine:

- `cargo test --locked`: **38 passed, 0 failed**
- `cargo clippy --all-targets --locked -- -D warnings`: **clean**
- `cargo fmt --all -- --check`: **clean**
- `cargo build --release --locked`: **success**

Verification transcript (abridged):

```
$ cargo build --release --locked
    Finished `release` profile [optimized] target(s)

$ cargo test --locked
running 38 tests
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo clippy --all-targets --locked -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo fmt --all -- --check
(no output)

$ python3 scripts/measure-startup.py target/release/lcars-mon 7
min 38.4 ms  median 39.1 ms  max 42.6 ms

$ python3 scripts/measure-idle.py target/release/lcars-mon 8
window: 8.00s
idle CPU: 0.62% of one core
RSS: 5176 KiB
terminal output: 5729 bytes/s
```

Tests cover:

- deterministic rate arithmetic, counter resets, interface appearance and
  disappearance, zero and very long intervals
- deterministic top-process selection and bounded sample histories
- layout rectangles at 80x24, 120x36, 110x34, 110x35, 112x28, 112x35, 200x60,
  and tiny sizes
- terminal-independent primary, diagnostics, help, ASCII, long-hostname, and
  non-ASCII process-name rendering
- presenter diff behavior
- clock and formatting helpers
- key mapping

## Performance

Verification environment: Linux `7.0.0-38-generic`, x86_64, 24 logical cores,
`rustc 1.99.1`, release build.

| Measurement | Result |
| --- | --- |
| Startup to first rendered frame (PTY, 7 runs) | min **38.4 ms**, median **39.1 ms**, max **42.6 ms** |
| Idle CPU (8 s window, one core) | **0.62%** |
| Resident memory | **5,176 KiB** |
| Terminal output while idle | **~5.7 KB/s** |
| Release executable | **823,688 bytes (804 KiB)** |
| Sample interval | **1.0 s** |
| Animation tick | **100 ms (10 fps)** |

Reproduce on Linux:

```sh
cargo build --release --locked
python3 scripts/measure-startup.py target/release/lcars-mon 7
python3 scripts/measure-idle.py target/release/lcars-mon 8
ls -l target/release/lcars-mon
```

On macOS, use the same startup script (PTY-based); for CPU/RSS run the app and
use `ps -o pid,%cpu,rss,etime,comm -p $(pgrep -n lcars-mon)` in another
terminal. `scripts/measure-idle.py` is Linux-specific because it reads `/proc`.

## Visual acceptance test (120x36)

The primary capture was assessed against the brief:

1. **Asymmetric silhouette** — yes. A five-column left spine, four unequal top
   rail segments, and a right elbow field create a deliberately unbalanced
   frame.
2. **Immediately dominant focal element** — yes. The 71-column amber clock is by
   far the largest readout.
3. **At least 20% intentional negative space** — yes. The central field around
   the clock, the load/trace field, and the lower-left void leave well over 20%
   black.
4. **Metrics integrated, not interchangeable boxes** — yes. Time is open, load
   lives in the negative space, memory/disk/network are rails and tags, and the
   CPU field is an elbow growing out of the header.
5. **Recognizable with text removed** — yes. The color rail L, the large digit
   silhouette, the right blue field, the memory rail, and the footer tags still
   frame the screen.
6. **Materially different from top/htop/btop** — yes. There are no bordered
   widgets, no conventional bar-chart grid, and no dashboard box layout.
7. **Process list absent until opened** — yes. The primary view contains no PID,
   process name, or process table; only `p`/`Tab` reveals the manifest.

## Known limitations

- **macOS was not executed in this environment.** The code uses only
  cross-platform crates and POSIX calls available on macOS, but the macOS path
  has not been run on hardware. This is the main unverified claim.
- Process CPU percentages follow `sysinfo` semantics and can exceed 100% for a
  multi-threaded process (the UI bar clamps the visual fraction; the number is
  shown as reported). Process memory is resident/footprint memory depending on
  the OS.
- The process list is refreshed every five seconds while the command view is
  active and every sample while diagnostics is open, so its CPU share is a
  short rolling window rather than an instantaneous instruction count.
- Network rates aggregate all interfaces, including loopback and container
  interfaces. Per-interface selection is intentionally out of scope.
- Disk usage is for `/` only, as required; other mounts are not scanned.
- On very narrow or short diagnostics layouts, some per-core entries can be
  replaced by a `+N CORES NOT SHOWN` note. The 120x36 view shows all 24 cores on
  the test machine; the 80x24 view shows up to 24.
- Swap is reported as `NO SWAP` when the host has none. Unsupported metrics show
  `N/A`/`SYNC` rather than fabricated values.
- `TERM=dumb`, non-TTY stdin, and redirected stdout are refused with actionable
  messages; use `--snapshot` for non-interactive output.
- No configuration file, mouse handling, process actions, themes beyond the
  explicit color/reduced modes, or plugin system was added.
- The interface is an original LCARS-inspired composition and is not affiliated
  with or copied from any franchise.

## Additional captures

- [80x24 command](captures/primary-80x24.png)
- [80x24 diagnostics](captures/diagnostics-80x24.png)
- [120x36 help panel](captures/help-120x36.png)
- [ASCII fallback, 120x36](captures/primary-ascii-120x36.txt)
- [Raw ANSI frames](captures/primary-120x36.ansi)

The `.txt`, `.html`, and `.ansi` files next to each PNG are the canonical
terminal output. The compact 80x24 views deliberately simplify rather than
compress the 120x36 composition.

## Deliverables

- Source, `Cargo.toml`, and `Cargo.lock`
- Tests in the library modules
- `captures/primary-120x36.png` (also `.html`, `.txt`, `.ansi`)
- `captures/diagnostics-120x36.png`
- `captures/primary-80x24.png` and `captures/diagnostics-80x24.png`
- `captures/help-120x36.png`
- `CAPTAINS_LOG.md`
