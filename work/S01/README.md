# lcars — LCARS-inspired terminal system monitor

A terminal command display for an engineering workstation: a thick orange
rail sweeping through an elbow, segmented colour blocks carrying short labels,
a dominant block-glyph clock, braille data traces, and a lot of deliberate
black. It is a real monitor underneath (CPU, load, memory, disk, network,
uptime, per-core, swap, process count, top processes) but the composition
comes first.

![primary view at 120x36](captures/primary-120x36.png)

## Language decision

**Rust**, with two dependencies. The criteria were fast startup, low idle
cost, smooth rendering and reliable macOS + Linux support:

* A statically linked native binary starts in well under a millisecond after
  `exec` (measured 0.4–0.5 ms to the first flushed frame) and idles at 0% CPU
  with a 2.8 MB resident set. No runtime, no interpreter warm-up, no GC.
* Both platforms' real APIs (`/proc`, `statvfs`, Mach `host_statistics64`,
  `sysctl`, libproc, routing sockets) are reachable through the `libc` crate
  without writing C or shelling out.
* The whole rendering path (cell buffer, diff, ANSI output) is plain Rust,
  so frame cost is a few microseconds and the output stream per idle second is
  about 2.5 KB.

Go or Zig would also have been credible; Rust won on the maturity of the
terminal-input crate and on being able to keep the dependency list to two
items while still getting signal-safe raw-mode handling.

### Third-party libraries

| Crate | Why |
| --- | --- |
| `crossterm 0.29` (features: `events` only) | Raw mode on/off, keyboard and resize events with escape-sequence parsing. Its styling API is **not** used; all output is hand-written ANSI from the diff renderer. Pulled in transitively: `mio`, `signal-hook`, `parking_lot`, `rustix`, `bitflags`. |
| `libc 0.2` | Direct OS calls: `statvfs`, `gethostname`, `uname`, `localtime_r`, `sysconf`, and on macOS `sysctl`, `host_statistics64`, `host_processor_info`, `proc_listpids`, `proc_pidinfo`, `proc_name`. |

Nothing else: no date crate (local time via `localtime_r`), no system-info
crate (collectors are written against the OS APIs directly), no TUI framework.

## Architecture

```
src/
  main.rs            CLI parsing, headless --snapshot mode, exit codes
  app.rs             App state machine + main loop (sample / render / wait / input)
  metrics/
    mod.rs           Snapshot, StaticInfo, Collector trait, platform selection
    rates.rs         RateTracker (elapsed-time based, counter wrap/reset aware), cpu_fraction, History
    sampler.rs       Sampler: raw Snapshot -> Telemetry (percentages, rates, top processes)
    units.rs         Fixed-width number formatting
    linux.rs         /proc parsers (pure functions) + statvfs
    macos.rs         Mach / sysctl / libproc / NET_RT_IFLIST2 collector
  term/
    buffer.rs        Cell buffer with clipping, wide-char handling, truncation
    color.rs         Palette, gradients, true-colour detection, 256-colour quantisation
    glyphs.rs        Unicode and ASCII glyph tables, braille composition
    render.rs        Diff renderer: emits only changed cells as ANSI
    session.rs       Raw mode, alternate screen, cursor, signal + panic restoration
  ui/
    mod.rs           Scene (pure input data), View, Tier (size classes), Status
    frame.rs         Elbows, rails, bars, pills, sweeps
    font.rs          5-row and 3-row half-block bitmap fonts
    widgets.rs       Braille traces, gradient gauges, core banks, pulse
    primary.rs       Command view
    diag.rs          Diagnostic view (mirrored frame)
    help.rs          Help overlay
tests/render.rs      Headless rendering tests over views, tiers and sizes
scripts/measure.sh   Startup / CPU / RSS / size / output-bytes measurement
scripts/ansi2svg.py  Turns tmux colour captures into SVG for the docs
captures/            120x36 captures (txt, ansi, svg, png) + measurement logs
```

Design rules the code follows:

* **Collection is separate from presentation.** Collectors return a
  `Snapshot` of raw counters with every field `Option`al. The `Sampler`
  derives percentages and rates purely from pairs of snapshots and their own
  monotonic timestamps. Views receive a `Scene` of plain data and draw into a
  `Buffer`; nothing in `ui/` touches a terminal or libc.
* **One failed collector never takes down the display.** Each metric is read
  independently; a failure becomes `None` plus a warning shown in the help
  overlay, and the view draws a `──` unavailable state.
* **Rates use measured elapsed time**, never the nominal interval. Zero
  elapsed time is skipped without moving the baseline; a 64-bit counter that
  decreases is treated as a reset (one `None` sample); 32-bit counters would be
  unwrapped (tested, not currently needed by either collector). Network rates
  are tracked per interface so an interface appearing or vanishing cannot
  produce a spike.
* **The main loop is four small steps**: sample if due, render if due or
  dirty, compute the next wake time, poll input with that timeout.
* **Rendering is a diff.** The previous and next buffers are compared cell by
  cell; only changed cells are written, with cursor moves and SGR changes
  elided when redundant. A seconds tick rewrites a few dozen cells. A resize
  rebuilds the buffers and repaints once.
* **Structure is drawn in the foreground.** Rails are block glyphs in a
  foreground colour, not coloured spaces, so `--mono` keeps the silhouette and
  only labels on rails use a background (rendered as reverse video in mono).

## Views and composition

* **Command view (default).** Left rail with elbows top and bottom; the top
  bar is two rows (half-height rounded upper edge), the bottom bar one row.
  Rail blocks carry uptime, memory, load and disk as labels, and the memory
  and disk blocks fill from the bottom like a gauge. The clock is the focal
  readout; CPU gets a second, smaller block-font readout with a braille
  trace; memory/disk and RX/TX are paired sweeps. The lower third is empty by
  design apart from a dim sampling readout.
* **Diagnostic view (`p` or `Tab`).** The frame is mirrored (rail on the
  right) so the two views are unmistakably different. Process count is the
  focal readout; then OS/kernel/arch, a per-core bank (two-column bars with
  labels, packing tighter when there are more cores than columns), swap and
  sampling stats, and the five busiest processes as pill rows with PID, name,
  CPU bar/percent and RSS. The process list never appears in the command view.
* **Help overlay (`?`)** is an LCARS card listing every key, the active
  colour/glyph mode, the interval, and any collector warnings.
* **Size tiers.** 100x30 and up is the full composition; 60x16 to 99x29 is a
  deliberately simplified command display (the 80x24 layout drops the
  cores/peak column and the three-row network traces); below that a minimal
  frame with time, status, CPU, memory and load and a hint to enlarge. Every
  size down to 0x0 renders without panicking (tested).

## Controls

| Key | Action |
| --- | --- |
| `q`, `Esc`, `Ctrl-C` | quit (terminal state restored) |
| `?` | help overlay |
| `Space` | hold / resume sampling (clock keeps running, a HOLD pill appears) |
| `p` | open / close the process diagnostic view |
| `Tab` | cycle views |

The bottom bar always shows the key hints, and labels change with state
(`SPC HOLD` becomes `SPC RESUME`, `P PROC` becomes `P CLOSE`).

```
lcars [--interval MS] [--color auto|true|256|mono] [--mono] [--ascii]
      [--view command|diag] [--snapshot WxH] [--exit-after MS]
```

* Colour: `COLORTERM=truecolor|24bit` (or a known 24-bit terminal program)
  selects 24-bit output; otherwise 256 colours with nearest-cube quantisation;
  `NO_COLOR` or `TERM=dumb` selects monochrome. `--color` overrides.
* Glyphs: a UTF-8 locale (`LC_ALL`/`LC_CTYPE`/`LANG`) selects the Unicode
  set; otherwise, or with `--ascii`, the 7-bit set (`#`, `"`, `_`, `.`,
  `:`, `+`), and non-ASCII data such as process names is shown with `?`.
* `--snapshot 120x36` renders one frame as plain text to stdout without a
  terminal (used for captures and as a startup/sanity check); running the
  interactive mode with stdout redirected exits with an actionable error.

## Platform implementation

### Linux (`metrics/linux.rs`)

| Metric | Source |
| --- | --- |
| CPU total / per core | `/proc/stat` (busy = everything except idle and iowait) |
| Load averages | `/proc/loadavg` |
| Memory | `/proc/meminfo` (`MemAvailable`, with the pre-3.14 estimate as fallback) |
| Swap | `/proc/meminfo` `SwapTotal`/`SwapFree` (absent → unavailable) |
| Disk | `statvfs("/")` |
| Network | `/proc/net/dev`, loopback excluded, per-interface counters |
| Uptime | `/proc/uptime` |
| Processes | `/proc/<pid>/stat` parsed from the last `)` so names with spaces, parentheses or non-UTF-8 bytes survive; CPU time from utime+stime over `CLK_TCK`, RSS from pages × page size; count = numeric entries in `/proc` |
| Identity | `gethostname`, `uname`, `/etc/os-release` |

The process scan only runs while the diagnostic view is open.

### macOS (`metrics/macos.rs`)

| Metric | Source |
| --- | --- |
| CPU total | `host_statistics64(HOST_CPU_LOAD_INFO)` |
| Per core | `host_processor_info(PROCESSOR_CPU_LOAD_INFO)` + `vm_deallocate` |
| Load averages | `getloadavg` |
| Memory | `hw.memsize` and `host_statistics64(HOST_VM_INFO64)`; used = (active − purgeable) + wired + compressed, as Activity Monitor reports it |
| Swap | `sysctl vm.swapusage` (`xsw_usage`) |
| Disk | `statvfs("/")` |
| Network | `sysctl NET_RT_IFLIST2` → `if_msghdr2.ifm_data` (64-bit counters; `getifaddrs` only offers 32-bit ones), loopback and down interfaces excluded |
| Uptime | `sysctl kern.boottime` |
| Processes | `proc_listpids(PROC_ALL_PIDS)`, `proc_pidinfo(PROC_PIDTASKINFO)` for CPU ns and RSS, `proc_name`; other users' processes that return `EPERM` are skipped silently |
| Identity | `gethostname`, `uname`, `kern.osproductversion` |

No elevated privileges are needed on either platform. The macOS collector was
type-checked and linted for `aarch64-apple-darwin` on this Linux machine; see
*Known limitations*.

## Building

Reproducible with a stable Rust toolchain (1.80 or newer; `Cargo.lock` is
committed, `rust-toolchain.toml` pins the stable channel):

```sh
cargo build --release            # target/release/lcars (~670 KB, stripped, LTO)
./target/release/lcars
```

Cross-check from Linux for macOS (no linker needed for `check`/`clippy`):

```sh
rustup target add aarch64-apple-darwin
cargo check --target aarch64-apple-darwin
```

No network is needed at run time; the build needs the two crates and their
transitive dependencies from crates.io.

## Testing, linting and measurement

```sh
cargo test                                   # 53 unit + 10 headless rendering tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo clippy --target aarch64-apple-darwin --all-targets -- -D warnings
./scripts/measure.sh                         # size, startup, idle CPU/RSS, bytes/s
```

What the tests cover:

* **Sampling / rates**: elapsed-time-based rates, zero elapsed, hour-long
  gaps, 64-bit resets, 32-bit wraps, CPU fractions with resets, bounded
  histories, per-interface tracking with appearing/vanishing interfaces,
  process ranking with vanishing PIDs, top-5 cap, and every `/proc` parser on
  fixtures including a hostile process name (`my (weird) proc ñ日本`).
* **Layout**: segment and rail-block distribution, size tiers, frame drawing
  at sizes from 0x0 to 200x60 on both sides.
* **Terminal-independent rendering** (`tests/render.rs`): every required
  metric present at 120x36, process list absent from the command view and
  present in the diagnostic view, ≥20% negative space, asymmetric silhouette
  measured on block glyphs only, ASCII mode emitting only ASCII, unavailable
  states, and every view at 19×16 sizes without overflow or panic.
* **Diff renderer**: a one-cell change writes one cell; a seconds tick
  rewrites fewer than 60 cells; mono output contains no colour sequences.

Results from this machine (24-core Linux, see `captures/verification.txt`
and `captures/measurements.txt`):

| Measurement | Result |
| --- | --- |
| Executable | 670,704 bytes (stripped, LTO) |
| Process start → first frame flushed, real pty | 0.4–0.5 ms (5 runs) |
| Idle CPU over 30 s at 120x36 (1 s sampling, 4 fps animation) | 0% (0.01 s user, 0.00 s system) |
| Max resident set | 2,812 KB |
| Terminal output while idle | ~42 writes / 2.5 KB per second |
| `cargo test` | 63 passed, 0 failed |
| `cargo clippy -D warnings` (Linux and macOS targets) | clean |

The "first frame" number is measured from the top of `main`, so it excludes
`exec` and dynamic-loader time (a further ~1 ms). `--exit-after 0` prints it
for any terminal.

## Captures

`captures/primary-120x36.*` and `captures/diagnostic-120x36.*` were taken
from the release binary running in a 120x36 tmux pane with
`COLORTERM=truecolor` (`.ansi` is the raw colour capture, `.txt` plain text,
`.svg`/`.png` rendered from the `.ansi` with `scripts/ansi2svg.py`).
`captures/primary-80x24.txt` and `captures/help-120x36.txt` show the
simplified tier and the overlay.

![diagnostic view at 120x36](captures/diagnostic-120x36.png)

Visual acceptance at 120x36: silhouette asymmetric (left rail + elbows, open
right edge) — yes; one dominant focal element (the clock) — yes; negative
space ≥ 20% — yes (about a third of the cells are blank, enforced by a
test); metrics integrated rather than boxed — yes (rail labels, sweeps,
gauges; no bordered widgets); recognisable with text removed — yes (tested
on block glyphs only); materially different from top/htop/btop — yes;
process list absent until the diagnostic view opens — yes (tested).

## Known limitations

* **macOS is compiled and linted, not executed.** This project was built on
  Linux; the Darwin collector passes `cargo check` and `clippy` for
  `aarch64-apple-darwin` but has not run on a Mac. The APIs used are the
  documented ones that Activity Monitor-style tools rely on; the most likely
  divergence is in the memory "used" definition, which is a judgement call on
  macOS.
* **Per-process CPU on macOS counts only processes the user may inspect.**
  `proc_pidinfo` returns `EPERM` for other users' processes without root, so
  root-owned daemons do not appear in the top five. The process *count*
  still includes them.
* **Disk is the root filesystem only.** Other mounts are not shown.
* **Wide-character width is heuristic.** CJK, Hangul and common emoji
  ranges are treated as two columns; unusual scripts may misalign a process
  name by a column.
* **True colour is detected, not probed.** Terminals that support 24-bit
  colour but advertise neither `COLORTERM` nor a known `TERM_PROGRAM` get
  256-colour quantisation; pass `--color true`.
* **The animation runs at 4 fps** to keep the seconds display and the pulse
  moving; this costs about 2.5 KB/s of terminal output. Holding (`Space`)
  stops sampling but not the clock.
* **Windows is out of scope.**
