# Verification record

Environment: Linux 6.8.0-139-generic x86_64 (24 logical CPUs, 93 GiB RAM),
rustc 1.98.1, recorded 2026-09-22. All numbers below were measured on this
machine. Nothing is estimated.

## Build, lint, test

```sh
./scripts/check.sh
```

| Step | Command | Result |
|---|---|---|
| Format | `cargo fmt --check` | clean |
| Lint (Linux) | `cargo clippy --all-targets -- -D warnings` | 0 warnings |
| Lint (macOS) | `cargo clippy --target aarch64-apple-darwin -- -D warnings` and `x86_64-apple-darwin` | 0 warnings (type-check only; not linked or run) |
| Tests | `cargo test` | **77 passed**, 0 failed (60 unit, 17 rendering integration) |
| Release | `cargo build --release` | OK, stripped binary 602,864 bytes (588 KiB) |

What the tests cover:

- **Sampling and rates** (`metrics/sampler.rs`): rates use measured elapsed
  time; zero and backwards intervals keep previous rates and baseline; a
  one-hour gap yields the correct average; 32-bit wrap versus reset;
  interfaces resetting, appearing and disappearing; CPU counter reset;
  processes exiting; PID reuse; top-5 cap; per-collector failure isolation.
- **Linux parsers** (`metrics/linux.rs`): `/proc/stat`, `/proc/meminfo`
  (with and without MemAvailable), `/proc/net/dev`, and `/proc/<pid>/stat`
  with hostile names containing `)` and spaces, plus non-ASCII names.
- **Layout** (`ui/layout.rs`): at every size from 44×14 to 260×90, content
  regions fit on screen, never overlap and stay clear of the sidebar; tiers
  switch at the reference sizes; larger terminals grow the clock.
- **Terminal-independent rendering** (`tests/render.rs`):
  - no panic at any size from 1×1 to 218×70, in both views, with and without
    help, and with all metrics unavailable
  - at least 20 % negative space
  - the process list is absent from the command view at every size
  - all required readouts are present in both views
  - hostile names are sanitized and ellipsized, with no orphaned wide-char
    halves
  - help lists every control
  - focus changes the trace
  - HOLD is visible
  - the silhouette survives with the text removed and is asymmetric
  - ASCII+mono output is pure ASCII without colour codes
  - all colour modes place identical cells
  - an unchanged frame emits 0 bytes
  - a clock tick only repaints the clock region
- **Encoder, canvas, raster and font:** minimal diffs, differential SGR,
  wide characters, quadrant and Braille mapping, and stroke-font glyphs that
  are all distinct.

## Real-terminal lifecycle (pty)

```sh
uv run --with pyte scripts/pty_harness.py lifecycle ./target/release/lcarsmon
```

The harness runs the release binary in a pseudo-terminal. For each scenario
it checks the exit code, that the pty is back in cooked mode (ICANON and
ECHO), and that the cursor-show and alternate-screen-leave sequences were
emitted.

```
PASS  quit with q          exit=0 cooked=True restore-seq=True
PASS  quit with Esc        exit=0 cooked=True restore-seq=True
PASS  Ctrl-C               exit=0 cooked=True restore-seq=True
PASS  SIGTERM              exit=0 cooked=True restore-seq=True
PASS  SIGINT               exit=0 cooked=True restore-seq=True
PASS  SIGHUP               exit=0 cooked=True restore-seq=True
PASS  resize storm + keys  exit=0 cooked=True restore-seq=True
PASS  Ctrl-Z then fg       exit=0 cooked=True restore-seq=True
```

The resize storm goes 80×24 → 30×8 → 200×60 → 1×1 → 120×36 with SIGWINCH,
then presses `p ? Tab Space` and an arrow-key escape sequence. The Ctrl-Z
scenario runs under a tiny job-control shell. It checks that the terminal is
restored *before* the stop and re-entered after SIGCONT.

Error paths:

```
$ lcarsmon | cat
lcarsmon: stdout is not a terminal. Run lcarsmon in an interactive terminal, or use `lcarsmon --snapshot command` to print a single frame.
$ lcarsmon --interval 5
lcarsmon: --interval must be between 250 and 10000 ms, got 5        (exit 2)
```

## Performance

```sh
./scripts/perf.sh 30
```

```
binary size: 602864 bytes (588 KiB, stripped release)
time to first full frame over 20 runs: median 1.9 ms, min 1.9 ms, max 2.3 ms
command view, 30s run: CPU 0.055s total = 0.18% of one core; peak RSS 3.1 MiB; output 25557 B in the first second, then 4420 B/s
diagnostic view, 30s run: CPU 0.208s total = 0.69% of one core; peak RSS 3.4 MiB; output 35921 B in the first second, then 1085 B/s
```

How each figure is measured:

- **Startup:** spawn on a pty until the first frame's synchronized-output end
  marker arrives. The 250 ms target is met with roughly 100× headroom.
- **CPU:** `wait4()` rusage (user + sys) over the run, divided by wall time.
- **Peak RSS:** the app's own `VmHWM` (Linux) or sampled `ps` RSS (macOS).
  `ru_maxrss` isn't used because it includes the pre-exec Python fork.
- **Output:** bytes written to the pty after the first full paint.

The diagnostic view costs more because it scans the full process table
(about 490 processes) once per interval. The command view never scans
processes.

## Captures (`docs/captures/`)

The `.ansi`/`.png`/`.txt` files for `primary-120x36`, `diagnostic-120x36`,
`primary-80x24` and `diagnostic-80x24` are **live**. The interactive binary
ran for 60 s in a pty while short, real CPU bursts were generated (visible in
the CPU trace). Its byte stream was fed through the `pyte` terminal emulator,
and `p` was pressed for the diagnostic capture:

```sh
uv run --with pyte scripts/pty_harness.py capture ./target/release/lcarsmon /tmp/cap120 120x36 60 p
uv run --with pillow scripts/ansi2png.py docs/captures/primary-120x36.ansi docs/captures/primary-120x36.png
```

`primary-120x36-ascii-mono.*` and `primary-120x36-ascii.txt` show the
degraded fallback (`--snapshot command --ascii --mono`).

## Visual acceptance test (primary view, 120×36)

| # | Question | Answer | Evidence |
|---|---|---|---|
| 1 | Is the silhouette asymmetric? | **Yes** | A heavy left sidebar with two elbows (upper frame curving down-left, lower frame curving up-left), rails running right, and a gauge only on the lower right. In the automated check, 47 % of shape cells have no mirror twin (a symmetric layout scores 0 %). |
| 2 | Is one element immediately dominant? | **Yes** | The 7-row, 56-column orange clock. Load digits are about half its height and the condition plate is 2 rows. |
| 3 | Is at least 20 % intentional negative space? | **Yes** | 54.5 % of cells are pure background in the live capture (54.3 % at 80×24). The test enforces at least 20 %. |
| 4 | Are metrics integrated rather than boxed? | **Yes** | Values are cut into sidebar blocks and rails, the gauge carries its own legend under the ring ends, and the ledger sits inside the upper frame's header. There are no bordered widget boxes. |
| 5 | Is the layout recognizable with the text blanked? | **Yes** | Elbows, segmented rails, stacked sidebar blocks, clock numerals (pixels, not text), concentric arcs, and the key-pill rail. A test checks that the frame covers more than 20 % of the screen with the text ignored. |
| 6 | Is it materially different from top, htop and btop? | **Yes** | No process table, no per-core bar grid, no boxed panels. It's a starship-console composition with one focal clock. |
| 7 | Is the process list absent until requested? | **Yes** | It's absent at every tested size, enforced by a test, and the process table isn't even collected until `p` is pressed. |

Revisions made during review:

- A one-row sidebar sliver is now merged into the previous block.
- Elbow curves got larger radii.
- The condition readout became a 2-row plate.
- Load digits were shrunk so they no longer compete with the clock at large
  sizes.
- The gauge is suppressed when too small for its legend (100×30 now gets a
  full-width trace instead).
- The header keeps its NET line at 30 rows.
- ASCII downsampling was changed to keep the gaps between sidebar blocks.
