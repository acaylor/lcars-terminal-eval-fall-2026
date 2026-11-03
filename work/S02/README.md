# lcarsmon

An LCARS-inspired terminal system monitor for Linux and macOS. It's an
original interpretation of the starship-console look: asymmetric elbows,
thick colour rails, pill controls, a dominant clock, and telemetry cut into
the frame. It's built as a real, low-overhead monitoring tool.

![Command view at 120x36](docs/captures/primary-120x36.png)
![Diagnostic view at 120x36](docs/captures/diagnostic-120x36.png)

Both images are live captures of the interactive binary, driven in a
pseudo-terminal and rendered through a terminal emulator (see
[Verification](docs/VERIFICATION.md)). Text versions sit next to them in
`docs/captures/`.

## Language decision: Rust

| Criterion | Why Rust wins here |
|---|---|
| Fast startup | Native binary with no VM or runtime. The first full frame reaches the terminal in about 2 ms. |
| Low CPU and memory | No GC. Idle is 0.18 % of one core with a 3.1 MiB peak RSS. |
| Smooth rendering | Full control over the byte stream: a double-buffered cell diff, differential SGR, and synchronized output (DEC 2026). |
| macOS and Linux | Direct, zero-cost FFI to `/proc`, Mach host statistics, `sysctl`, and `libproc`, with compile-time platform isolation (`#[cfg]`). |

Go was the runner-up: comparable portability, but a GC runtime, a larger
binary, and less direct access to Mach/`libproc` without cgo. C/Zig could
match the performance, but at a real cost in memory safety for parsing
hostile process names.

### Third-party libraries (two)

| Crate | Why |
|---|---|
| `libc` | Raw bindings for termios, poll, signals, `ioctl(TIOCGWINSZ)`, `statvfs`, `sysctl`, Mach (`host_processor_info`, `host_statistics64`), and `libproc`. Every system call is ours. |
| `unicode-width` | Correct terminal column widths for CJK and emoji process and host names. Getting this wrong corrupts layouts. |

I deliberately use no TUI framework (ratatui, crossterm). The design depends
on sub-cell rasterization (quadrant and Braille), a custom diff encoder, and
precise lifecycle control, so a widget toolkit would add weight without
helping.

## Architecture

```
metrics/            OS → raw counters          (no presentation)
  mod.rs            Collector trait, RawSample, Reading<T> = Result<T, Missing>
  linux.rs          /proc/stat, /proc/meminfo, /proc/net/dev, /proc/<pid>/stat, statvfs, CLOCK_BOOTTIME
  macos.rs          host_processor_info, host_statistics64, sysctl (hw.memsize, vm.swapusage,
                    kern.boottime, NET_RT_IFLIST2), libproc (proc_listallpids / PROC_PIDTASKALLINFO)
  unsupported.rs    portable subset for other Unixes
  sampler.rs        raw → rates; pure & deterministic (time is a parameter)
  history.rs        fixed-capacity ring buffers (bounded memory)
canvas.rs           cell grid, clipping, wide-char handling  ─┐
raster.rs           quadrant (2×2) and Braille (2×4) rasters   │ terminal-free,
ui/                 Scene → Canvas: layout, font, frame, views ─┘ fully testable
ansi.rs             Canvas diff → escape sequences; colour/ASCII degradation
term.rs, run.rs     the only code that touches the tty: raw mode, signals, event loop
app.rs              key → action state machine, sample/compose orchestration
```

- **Collection is separated from presentation.** Collectors return raw
  counters. `Sampler::ingest(raw, at)` derives percentages and rates from the
  *measured* elapsed time. Intervals under 50 ms keep the previous rates and
  baseline, so there's never a division by zero. Very long gaps average
  correctly. Counter decreases count as a reset (skipped), or as a wrap
  when a counter modulus is known and the implied delta is plausible. New
  and vanished interfaces are handled per name. Process CPU% is keyed by
  (pid, start time), so reused PIDs never inherit counters.
- **Failure isolation.** Every metric is a `Reading<T>`, and a failed
  collector yields `N/A`, `DENIED`, `FAULT` or `SYNC` in the UI, never a
  crash. Nothing is fabricated.
- **Rendering.** Frame shapes (elbows with convex outer corners and concave
  fillets, rails, blocks) are painted into one screen-sized 2×2 sub-pixel
  raster and converted to quadrant glyphs. Labels are then cut into the solid
  cells. Big numerals come from a small scalable stroke font rasterized by
  distance field. Traces use Braille.
- **Event loop.** `poll()` on stdin plus a self-pipe for signals. Frames are
  aligned to 200 ms wall-clock ticks (so the seconds digit flips on time) and
  samples run every interval (default 1 s). Unchanged frames emit zero bytes.
  The process table is scanned only while the diagnostic view is open.

## Views

**Command view (default).** A double-elbow frame sits on a heavy left
sidebar. The dominant focal element is the clock (HH:MM:SS). Secondary focal
readouts are the load average and a condition plate (NOMINAL / ELEVATED /
RED ALERT, derived only from real readings). The header ledger shows memory
used/available/total, root disk, and network RX/TX. Sidebar blocks carry
CPU, MEM, DSK, NET and LOAD. A sensor trace follows the Tab-focused
subsystem (CPU, MEM, NET mirrored RX/TX, LOAD). A concentric sweep gauge
shows CPU, MEM and DSK with the swap state at its hub. Animated elements
include the scanner on the lower rail, the pulsing live indicator, the
blinking HOLD and RED ALERT states, and the scrolling traces.

- **80×24 and similar:** a deliberately simpler single "[" frame, a smaller
  clock, and a compact ledger. No gauge.
- **≥100×30:** the full double frame, header ledger, and sweep gauge. The
  clock, gauge and trace grow on larger terminals.
- **Below 44×14:** a clipped, still-live minimal readout. It never panics,
  down to 1×1.

**Diagnostic view (`p`).** A mirrored, cool-toned frame with its sidebar on
the right. It shows host, OS, kernel, architecture, logical CPUs and uptime;
a per-core comb; a large process count with a restricted count; the swap
plate; and the top five processes by CPU (PID, name, CPU% with rail, RSS and
MEM%).

## Controls

| Key | Action |
|---|---|
| `q`, `Esc`, `Ctrl-C` | Quit (`Esc` closes the help panel first when it's open) |
| `?` (or `h`) | Help overlay |
| `Space` | Hold/resume telemetry (the clock keeps running; HOLD blinks) |
| `p` | Open/close the diagnostic view |
| `Tab` / `Shift-Tab` | Cycle sensor focus; from the diagnostic view, return to command |
| `Ctrl-Z` | Suspend (the terminal is restored first), resume with `fg` |

The bottom rail of every view doubles as a key legend.

## Build, run and test

The project needs Rust 1.75 or newer. After the first build, no network is
needed (only `libc` and `unicode-width` are fetched then).

```sh
cargo build --release            # binary: target/release/lcarsmon (~590 KiB, stripped)
./target/release/lcarsmon        # run
cargo test                       # unit + terminal-independent rendering tests
./scripts/check.sh               # fmt --check, clippy -D warnings (Linux + macOS targets), tests, release build
./scripts/perf.sh [SECONDS]      # binary size, startup, idle CPU, peak RSS, output rate (python3 stdlib only)
```

Reproducible builds: `Cargo.lock` pins exact dependency versions. Use
`cargo build --release --locked`. For macOS from macOS:
`cargo build --release --locked --target aarch64-apple-darwin` (or
`x86_64-apple-darwin`). From Linux, `cargo clippy --target
aarch64-apple-darwin` type-checks the macOS collector. Linking needs the
macOS SDK.

### Options

```
-i, --interval <MS>     sampling interval 250–10000 ms (default 1000)
    --color <MODE>      auto | truecolor | 256 | 16 | mono
    --mono              monochrome (also: NO_COLOR=1)
    --ascii / --unicode force ASCII-safe glyphs / force Unicode
    --snapshot <VIEW>   print one frame (command | diagnostic) and exit; works without a tty
    --size <COLSxROWS>  snapshot size (default 120x36)
    --ansi              colours in snapshots
    --exit-after <MS>   quit automatically (used by the measurement scripts)
```

Colour is detected progressively. `COLORTERM=truecolor|24bit` and known
terminals get 24-bit colour with gradients. `TERM=*256*` gets xterm-256.
Other terminals get 16 colours, and `NO_COLOR` or `TERM=dumb` get mono.
Everything is drawn in RGB and quantized only at emission, so layout is
identical in every mode. ASCII mode switches on automatically for
non-UTF-8 locales. Mono uses reverse video for frame shapes.

Redirected output or an unusable terminal gets an actionable message and a
non-zero exit (for example, "stdout is not a terminal … use `lcarsmon
--snapshot command`").

## Platform notes

- **Linux.** Reads `/proc` and `/sys` only, with no privileges. Network rates
  sum interfaces backed by a device (`/sys/class/net/*/device`) so that
  docker/veth bridges don't double-count. If none exist, it falls back to all
  non-loopback interfaces. Process names come from `comm`, extended from
  `argv[0]` when the kernel truncated them.
- **macOS.** CPU comes from `host_processor_info`. "Memory used" matches
  Activity Monitor (app + wired + compressed). The disk reading is the APFS
  Data volume (what users think of as `/`), swap comes from `vm.swapusage`,
  network uses 64-bit `if_data64` counters, and process CPU time is converted
  from Mach ticks with `mach_timebase_info`.

## Known limitations

- **macOS runtime verification.** The macOS collector compiles and passes
  clippy for `aarch64-apple-darwin` and `x86_64-apple-darwin`, but it was
  not executed in this environment (Linux host). Its logic follows the
  documented Mach, sysctl and libproc APIs, but treat it as untested until
  it has run on a Mac.
- **Other users' processes on macOS.** Without privileges, macOS refuses
  task info for other users' processes. They are counted and shown as
  "RESTRICTED", but can't be ranked by CPU.
- **Process ranking warm-up.** The first process ranking after opening the
  diagnostic view needs a second sample (about 0.3 s). Until then the
  CPU column reads "CALIBRATING".
- **Rounded shapes** are approximated at 2×2 sub-cell resolution. The look
  depends on the font rendering block elements seamlessly. Most modern
  terminals do; some fonts show hairline seams.
- **Synchronized output (DEC 2026)** is ignored by terminals that don't
  support it. The diff renderer keeps flicker minimal there as well.
- **Idle output** is about 4.4 KB/s in the command view. It comes from the
  clock, the pulse, and the rail scanner animation, and is fine even over
  SSH.
- **Load average on macOS** counts runnable threads differently from Linux,
  so values aren't directly comparable across platforms.

See [docs/VERIFICATION.md](docs/VERIFICATION.md) for the exact commands and
results, and [docs/CAPTAINS_LOG.md](docs/CAPTAINS_LOG.md) for the project
log.
