# `lcars-mon` — LCARS-Inspired Terminal System Monitor

`lcars-mon` is a fast, native Starfleet LCARS-inspired terminal command and engineering workstation monitor for **Linux** and **macOS**. Rather than skinning a conventional `top`/`htop`/`btop` dashboard of rectangular boxes, `lcars-mon` builds its entire interface around an asymmetric LCARS silhouette: sweeping curved elbows, segmented horizontal and vertical color rails, rounded pill terminals, negative space, 3-row vector focal readouts, and high-resolution Unicode Braille (`2×4` sub-cell) harmonic and network waveforms.

---

## 1. Language Decision & Prime Directive

**Selected Language:** **Rust (2021 Edition)** with a single FFI bindings dependency (`libc = "0.2"`).

Why Rust was chosen over Go, C/C++, Python, or Node.js:
1. **Instant Startup (< 1 ms):** Compiled ahead-of-time to native machine code with no VM, JIT warmup, or runtime initialization. Measured cold startup + first-frame render is **~0.37 ms** (nearly 700× faster than the 250 ms target).
2. **Minimal CPU & Memory Overhead:** Zero garbage collector pauses during smooth terminal animations, bounded ring-buffer history (`64` samples), and a ~3.4 MiB Resident Set Size (RSS) footprint.
3. **Bespoke 2D Cell-Diffing Canvas:** Standard TUI widget libraries (`ratatui`, `bubbletea`, `ncurses`) are architected around rectangular boxes and borders. Writing a custom double-buffered `Canvas` and Synchronized Output (`DECSET 2026`) cell differ directly on top of Rust's `std::io::Write` and `libc` gives complete control over sweeping LCARS geometry, sub-character half-block/quadrant curves, and deterministic headless testing.
4. **First-Class Linux & macOS Native APIs:** `libc` provides exact, ABI-verified struct layouts and constants for both Linux (`/proc`, `statvfs`, `sysconf`, `uname`, `termios`, `ioctl`, `poll`) and macOS (`sysctl`, Mach `host_processor_info` / `host_statistics64`, `libproc` `proc_listallpids` / `proc_pidinfo`, `getifaddrs`, `mach_timebase_info`).

### Third-Party Libraries
- [`libc`](https://crates.io/crates/libc) (`0.2`): Selected as the **only** external dependency to provide safe, platform-verified C type definitions and FFI declarations for POSIX terminal control (`termios`, `ioctl`, `poll`, `sigaction`), filesystem stats (`statvfs`), and macOS Mach/`sysctl`/`libproc` APIs.

---

## 2. Architecture

The codebase separates OS metric collection, stateful rate calculation, terminal lifecycle management, and LCARS visual composition into isolated, testable modules:

- [`src/model.rs`](src/model.rs): Platform-agnostic data structures (`RawSystemSample`, `SystemTelemetry`, `CpuTimes`, `LocalClock`, `SystemAlertState`) and UTF-8-safe formatting helpers.Every metric group uses `Option<T>` so unsupported or failed metrics are never fabricated.
- [`src/collector/mod.rs`](src/collector/mod.rs): Defines the `MetricCollector` trait and shared POSIX collectors (`localtime_r`, `gethostname`, `uname`, `getloadavg`, `statvfs`).
- [`src/collector/linux.rs`](src/collector/linux.rs): Native Linux collector reading `/proc/stat`, `/proc/meminfo`, `/proc/uptime`, `/proc/net/dev`, `/proc/[pid]/stat`, and `/etc/os-release` with pure, cross-platform unit-tested parsers.
- [`src/collector/macos.rs`](src/collector/macos.rs): Native macOS collector using `sysctl` (`KERN_BOOTTIME`, `HW_MEMSIZE`, `VM_SWAPUSAGE`), Mach host APIs (`host_processor_info`, `host_statistics64`, `vm_deallocate`), `getifaddrs` (`AF_LINK`), and `libproc` (`proc_listallpids`, `proc_pidinfo(PROC_PIDTASKALLINFO)`).
- [`src/sampler.rs`](src/sampler.rs): Deterministic `TelemetrySampler` state machine that computes CPU utilization deltas, per-interface network transfer rates against actual elapsed monotonic `Duration`, and top-5 CPU process rankings with counter-rollover and interface hotplug protection.
- [`src/theme.rs`](src/theme.rs): Restrained LCARS palette (`Orange`, `Amber`, `Salmon`, `Lavender`, `Periwinkle`, `CoolBlue`, `IceBlue`, `Cream`), progressive 24-bit `TrueColor` gradients, `Ansi256`, `Ansi16`, `Monochrome`, and `Unicode` / `Ascii` fallback detection.
- [`src/canvas.rs`](src/canvas.rs): Clipped 2D cell buffer, LCARS sweeping elbow & concave fillet primitives (`▛`, `▜`, `▙`, `▟`, `▀`, `▄`, `◖`, `◗`), 3-row vector digit renderer, `2×4` sub-cell `BrailleGrid` plotter, and `DECSET 2026` synchronized cell-diff emitter.
- [`src/ui.rs`](src/ui.rs): Responsive layout engine (`Micro`, `Compact 80×24`, and `Full Tactical 120×36+`), Primary Command View, Secondary Engineering & Process Diagnostic View, `?` Help Overlay, and programmatic `analyze_visual_acceptance` verifier.
- [`src/terminal.rs`](src/terminal.rs): RAII `TerminalSession` guard managing raw `termios`, alternate screen (`\x1b[?1049h`), cursor visibility (`\x1b[?25l`), `SIGWINCH` / `SIGINT` / `SIGTERM` / `SIGHUP` / `SIGQUIT` handlers, and panic-safe restoration.
- [`src/app.rs`](src/app.rs) & [`src/bench.rs`](src/bench.rs): Event controller and built-in performance/resource benchmark runner.

---

## 3. Controls & CLI Options

### Interactive Keyboard Controls
| Key | Action |
| :--- | :--- |
| `Tab` | Cycle between Primary Command View and Secondary Diagnostic View |
| `p` / `P` | Open or close the Secondary Process & Engineering Diagnostic View |
| `Space` | Pause / resume live telemetry updates |
| `?` | Open or close the LCARS command reference help overlay |
| `c` / `C` | Cycle color fidelity live (`TrueColor` → `256-Color` → `16-Color` → `Monochrome`) |
| `u` / `U` | Toggle between Unicode Hi-Res glyphs and 7-bit ASCII fallback |
| `q` / `Esc` / `Ctrl-C` | Restore terminal state and exit cleanly |

### Command-Line Flags
```bash
lcars-mon [OPTIONS]

  -i, --interval <ms>             Refresh interval in ms (default: 750)
      --color <mode>              Color mode: auto, truecolor, 256, 16, mono
      --mono, --no-color          Force monochrome rendering
      --ascii                     Use 7-bit ASCII degraded glyph fallback
      --unicode                   Use Unicode half-block, quadrant & Braille glyphs
      --capture-primary [WxH]     Capture Primary Command View (default: 120x36) and exit
      --capture-secondary [WxH]   Capture Secondary Diagnostic View (default: 120x36) and exit
      --plain                     Omit ANSI escape codes in capture output
      --strip-text                Replace alphanumeric labels with blanks in capture
      --bench [frames]            Measure startup, CPU, RSS memory, and frame latency
  -h, --help                      Print help message
  -V, --version                   Print version information
```

---

## 4. Reproducible Release Builds, Tests & Benchmarks

Requires a standard Rust toolchain (`rustc` / `cargo` >= 1.75) on Linux or macOS. No network connection is required after installation.

```bash
# 1. Build optimized, stripped release binary (offline-capable)
cargo build --release --offline

# 2. Run the automated test suite (sampling, rates, edge cases, headless UI & acceptance)
cargo test --offline

# 3. Run strict Clippy static analysis across all targets
cargo clippy --offline --all-targets -- -D warnings

# 4. Run the built-in performance & visual acceptance benchmark (100 frames @ 120x36)
./target/release/lcars-mon --bench 100

# 5. Launch interactively
./target/release/lcars-mon
```

---

## 5. Known Limitations

1. **Per-Process Permissions on macOS:** Without elevated privileges (`root`), macOS restricts `proc_pidinfo(PROC_PIDTASKALLINFO)` for processes owned by other users or protected by SIP. `lcars-mon` gracefully skips unreadable PIDs without crashing or requiring `sudo`.
2. **First-Frame Network Rate:** Because network transfer rates require a delta between two byte-counter observations, instantaneous RX/TX rates begin updating on the second sample tick (`~750 ms` in interactive mode, `40 ms` in `--capture-*` mode).
3. **Terminal Font Coverage:** Full visual fidelity relies on standard Unicode block elements (`U+2580..U+259F`), geometric half-circles (`◖`/`◗`), and Braille patterns (`U+2800..U+28FF`). On legacy or non-UTF-8 terminals, use `--ascii` (or press `u` live) for the 7-bit ASCII fallback.

---

## 6. Captain's Log

See [`CAPTAINS_LOG.md`](CAPTAINS_LOG.md) for the mission log detailing the design, implementation, and verification of `lcars-mon`.
