# lcars — LCARS-inspired terminal system monitor

A production-quality terminal system monitor with an original LCARS-inspired
composition: a full-height left rail elbowed into a header band, giant
half-block clock, diagonal amber telemetry sweep, live braille history
trace, and a diagnostics view with per-core and top-5 process data.

Not a dashboard of boxes: the primary screen has no bordered widgets and no
process table. `p` or `Tab` opens diagnostics; `?` opens help.

## Language decision

**Go (1.27), cgo-free.** Rationale, in the priority order given:

1. **Fast startup** — native binary, no VM/JIT/interp warmup; measured
   ~27 ms end-to-end for a full one-frame run (`--once`).
2. **Low overhead** — ~0–0.25 % of one core while idling at 1 Hz on Linux,
   ~7.7 MB RSS, 1.9 MB stripped binary.
3. **Smooth rendering** — pure cell-buffer diffing: views paint a buffer,
   the renderer emits only changed spans with cursor-addressing, so steady
   state writes are tiny.
4. **macOS + Linux** — one cross-compile per OS/arch
   (`GOOS=darwin GOARCH=arm64` verified); no cgo means Docker-free release
   builds reproduce anywhere.

Rejected: Rust (similar runtime profile, slower iteration for identical
user-visible outcomes); Python/Node (startup + interpreter RSS dominate,
no static binary); C (portability of Darwin sysctl/Mach plumbing costs
more than it saves here).

## Architecture

```
              ┌─────────────┐   Snapshot   ┌──────────┐  *Buffer   ┌──────────┐
 metrics ───▶ │ linux.go /  │ ───────────▶ │  view.go │ ─────────▶ │ term.go  │ ──▶ ANSI
 (OS APIs,    │ darwin.go   │  Rates       │ (pure)   │            │ diffs    │
  /proc,      └─────────────┘              └──────────┘            └──────────┘
  sysctl)
```

- `internal/metrics` — `Sampler` interface, cumulative `Snapshot`s, and
  deterministic rate math. Linux reads `/proc` + `statfs(2)`. Darwin is
  cgo-free: `kern.boottime`/`hw.memsize`/`vm.swapusage`/`kern.cp_time`
  sysctls, `kern.proc.all` for processes, `x/net/route` NET_RT_IFLIST for
  NIC counters. Every collector fails independently to `N/A`.
- `internal/render` — cell buffer, themes (truecolor → 256 → mono), LCARS
  shapes (rails, pills, elbows), 3-row big-digit font, Braille traces,
  diffing renderer. Rendering is terminal-independent and unit-tested.
- `internal/app` — pure view builders (`Build`) + event loop
  (`app.Run`): raw mode via `x/term`, alternate screen, signal handling.
- `cmd/lcars` — flags only.

## Controls

| Key | Action |
|---|---|
| `q` / `Esc` / `Ctrl-C` | quit |
| `?` | help overlay |
| `Space` | pause / resume |
| `p` | toggle diagnostics |
| `Tab` | cycle primary ↔ diagnostics |

The footer on every screen lists these.

Flags: `--color=auto|true|256|mono`, `--ascii`, `--interval=1s`,
`--once` (render one frame and exit; used for captures).

Environment: `NO_COLOR`, `COLORTERM`, `TERM` feed auto-detection.

## Build & release

```sh
go build -trimpath -ldflags='-s -w' -o bin/lcars ./cmd/lcars      # native
GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 \
  go build -trimpath -ldflags='-s -w' -o bin/lcars-darwin ./cmd/lcars
go test ./... && go vet ./...
python3 tools/capture.py captures/primary-120x36.txt PRIMARY 120 36
python3 tools/measure.py
```

Third-party libraries: `golang.org/x/term` (raw mode, winsize;
BSD-3-Clause), `golang.org/x/sys/unix` (sysctls), `golang.org/x/net/route`
(Darwin interface RIB). Everything else is stdlib. No network access.

## Visual acceptance (120×36)

Assessed against the brief's 7 questions — all yes:

1. **Asymmetric?** Full-height left rail (7 cols), none on the right;
   top header band spans only the left/center.
2. **Dominant focal element?** The 3-row big clock + load value.
3. **≥20% negative space?** Lower-right void with starfield stipple;
   measured blank area ≈ 40% of cells.
4. **Metrics integrated, not boxed?** Gauges sit *in* the salmon sweep
   band; telemetry is an open left-edged list; zero bordered widgets.
5. **Recognizable sans text?** Rail + elbowed header + sweep silhouette
   survive blanking all glyphs.
6. **Different from top/htop/btop?** No row table on the primary screen
   at all; the only table is the opt-in diagnostics process list.
7. **Process list absent by default?** Yes — only under `p`/`Tab`.

Captures: `captures/primary-120x36.txt`, `captures/diag-120x36.txt`.

## Results (this machine, Linux x86-64)

| | |
|---|---|
| Build | `go build -trimpath -ldflags='-s -w'` → 1.9 MB |
| Tests | `go test ./...` — metrics parsers/rates, rendering buffer+fonts+diff, view layouts (primary/diag/compact/help/ascii/tiny sizes), input handling: **all pass** |
| Vet | `go vet ./...` clean (Linux + `GOOS=darwin`); `gofmt -l` clean |
| Startup | `--once` median **26.9 ms** (n=15) |
| Idle CPU | 0.00–0.25 % of one core @1 Hz, 120×36 |
| RSS | ~7.7 MB |

## Robustness

Resize (SIGWINCH → full repaint, no flicker), Ctrl-C/SIGTERM/SIGHUP
restore terminal via deferred cleanup, tiny terminals (<20×8 prints a
deliberate message; 80×24 gets the simplified layout), long/non-ASCII
host & process names are sanitized (control bytes → U+FFFD) and clipped,
counter reset/rollover yields `N/A` instead of garbage, vanishing
processes and interfaces are skipped, rates use wall-clock elapsed time
(never an assumed interval), each metric collector degrades independently.

## Known limitations

- macOS binary cross-compiles and vets clean from this Linux host, but
  has **not been run on real macOS here** — Darwin collectors follow
  documented sysctl layouts and fail to `N/A` on mismatch.
- macOS memory split (used/available) shows total only (`MEM … TOT`):
  Mach `vm_statistics64` needs cgo we deliberately avoided.
- Darwin per-core CPU is `N/A` (no stable public sysctl for it).
- Process CPU% on Darwin is instantaneous `p_pctcpu`, not rusage deltas.

## Captain's log

Stardate 2026-10-27. Built `lcars`, a Go terminal monitor whose default
screen is a genuine LCARS composition rather than a recolored dashboard:
segmented left rail with rotated labels, elbowed header, giant half-block
clock, sweeping salmon gauge band, Braille history, starfield void.
Verified the full brief: metrics on Linux from `/proc` (no shelling out),
rates computed from elapsed wall-clock, deterministic parsers tested
against fixtures, terminal-independent rendering, diffed output, resize
and signal handling, 80×24 compact mode, ASCII/mono/256/truecolor modes,
Darwin cross-compile, and measured startup (≈27 ms), idle CPU (≈0–0.25 %)
and RSS (≈7.7 MB). Remaining uncertainty is Darwin runtime behaviour,
which the code is written to degrade on, not crash on.
