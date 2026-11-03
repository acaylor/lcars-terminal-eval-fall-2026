# LCARS / Engineering

A quiet, asymmetric command display for your workstation. The clock anchors a sweeping amber elbow; lavender load numerals and a small telemetry field leave deliberate open space. A separate diagnostic view contains the processor array and five busiest accessible processes.

![Primary command view, 120 columns by 36 rows](artifacts/primary-120x36.png)

## Build and run

Install **Go 1.23 or newer** and Make. There are **no third-party Go dependencies**, downloaded assets, or runtime network requirements. On macOS, install Apple's Command Line Tools as well: the collector uses cgo to access the system libraries shipped with macOS.

```sh
# Linux (amd64, arm64, or another architecture supported by Go)
CGO_ENABLED=0 make build
./bin/lcars

# macOS, built locally on Apple Silicon or Intel
CGO_ENABLED=1 make build
./bin/lcars
```

The release command is `go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o bin/lcars ./cmd/lcars`. Use the same Go version, target architecture, and (on macOS) Apple SDK for repeatable builds. VCS stamping is disabled so source archives and this workspace's incomplete `.git` directory build consistently. Set `GOTOOLCHAIN=local` to prevent Go from acquiring another toolchain. Installation needs no root access: the executable can live anywhere you can write.

## Why Go

Go produces a native executable with quick startup and modest resident memory, and provides portable signal handling, monotonic time, and concurrency without an interpreter. A small C boundary on macOS exposes Mach and libproc without shipping another library. Keeping the renderer in the standard library gives direct control over the silhouette and changed-cell output. There is no TUI framework, configuration system, or background service.

## Controls

| Key | Action |
| --- | --- |
| `q`, `Esc`, Ctrl-C | Exit and restore the terminal |
| `?` | Toggle the help overlay |
| Space | Freeze/resume the clock, telemetry, and animation |
| `p`, Tab | Switch command / process diagnostics |
| `[` / `]` | Previous / next processor page in diagnostics |

The frame contains the main controls; help explains the remaining details. The clock follows the OS local timezone, including the `TZ` environment variable. Diagnostics show process CPU relative to **one logical core**: a multithreaded process can exceed 100%. Overall CPU is 0–100% across the machine.

```sh
./bin/lcars --color mono              # monochrome, with the same silhouette
./bin/lcars --color 256               # or 16 / truecolor / auto
./bin/lcars --ascii                   # no Unicode output
./bin/lcars --interval 2s             # default 1s; range 250ms–10s
./bin/lcars --snapshot --size 80x24    # plain text; safe to redirect
./bin/lcars --snapshot --view diagnostics
```

`auto` recognizes `COLORTERM=truecolor`, `24bit`, direct-color TERM entries, and 256-color TERM entries, otherwise falling back to 16 colors. `NO_COLOR` selects monochrome unless explicitly overridden. The `C` and `POSIX` locales select ASCII automatically. Monochrome uses black/white ANSI fields to preserve the frame. An unsuitable TERM or redirected interactive I/O produces an actionable error without entering the alternate screen.

## Architecture and measurement semantics

| Component | Responsibility |
| --- | --- |
| `internal/metrics` | Stable `Collector` interface; independent OS collectors; pure interval and ranking calculations |
| `internal/ui` | Bounded cell canvas, quadrant elbows, large digits, Braille trace, responsive views, color selection, changed-cell ANSI output |
| `internal/terminal` | Termios, terminal dimensions, alternate screen, cursor and wrap state cleanup |
| `internal/app` | Single-owner UI state, bounded sampler worker, controls, signals, cadence |
| `cmd/capture`, `scripts` | Real-sample capture and verification tools; not needed to run the monitor |

Linux reads `/proc/stat`, `/proc/meminfo`, `/proc/uptime`, `/proc/loadavg`, `/proc/net/dev`, and (only in diagnostics) `/proc/<pid>/stat`. Root disk use comes from `statfs`. Process ticks use `AT_CLKTCK` from the kernel auxiliary vector; guest CPU time is not counted twice. Host identity is read once. A disappearing or denied process is skipped.

macOS uses `host_processor_info`, `host_statistics64`, `host_page_size`, `sysctl`, `statfs`, and libproc's `proc_listpids`/`proc_pidinfo`. Network counters come from the **64-bit** `NET_RT_IFLIST2` interface records. Mach buffers, ports, and C allocations are released on every sample. There are no per-refresh shell commands on either platform.

Linux memory used is total minus `MemAvailable`. macOS availability is explicitly a **reclaimable estimate**, free plus inactive pages; used is total minus that estimate. Root disk used excludes free blocks, while available excludes filesystem reservations, so used plus available need not equal total. APFS root values describe the mounted root filesystem, not a sum of all volumes.

Network RX/TX sum non-loopback interfaces; bridges, VPNs, and virtual interfaces may count traffic at multiple layers. Interface identities and process start times prevent reusing counters for a replacement object. Rates use monotonic elapsed time. Counter resets, zero/backward intervals, gaps over 30 seconds, and missing baselines display `--` until another valid sample arrives. Resuming from pause also establishes a fresh baseline. First-sample CPU/rates are unavailable rather than invented zeros.

The sampler has bounded channels and does not block terminal interaction. System samples default to once per second; the clock/indicator advances four times per second. Only changed cell spans are sent; an unchanged canvas emits nothing. No screen-clear command is used for refreshes. History retains 120 samples, only the latest two raw sample generations are needed, and processes are enumerated only while diagnostics is active (plus at most queued work during a view switch).

## Verification and captures

```sh
make test                   # go test -race ./...
make lint                   # go vet and gofmt check
make captures               # four seconds of real host samples
make performance            # stdlib Python PTY checks + two 12-second profiles
go test ./internal/ui -run '^$' -bench . -benchmem -count=1
GOMAXPROCS=2 go test ./internal/metrics -run '^$' -fuzz FuzzProcessParser -fuzztime=2s
```

See [VALIDATION.md](VALIDATION.md) for the exact executed commands, measured results, visual checklist, and portability boundaries. [CAPTAINS-LOG.md](CAPTAINS-LOG.md) records the final mission log.

- [Primary 120×36](artifacts/primary-120x36.png) · [SVG](artifacts/primary-120x36.svg) · [ANSI](artifacts/primary-120x36.ansi)
- [Diagnostics 120×36](artifacts/secondary-120x36.png) · [SVG](artifacts/secondary-120x36.svg) · [ANSI](artifacts/secondary-120x36.ansi)
- [Compact 80×24](artifacts/compact-80x24.png) · [ASCII 80×24](artifacts/ascii-80x24.png)

Captures are exports of the **same cell canvas** as the live terminal, fed by 40 real OS samples. SVG/PNG files are inspection artifacts; the application renders ANSI in the terminal. The optional PNG conversion script needs Pillow and a monospace font; neither is an application dependency. Capture metadata records the sample time, visible processes, interfaces, and negative space. This sandbox has only loopback networking and four visible processes, so the captures preserve the unavailable network state and show four process rows.

## Known limitations

- Native Linux execution is verified. macOS collectors are implemented against Apple's APIs; this Linux environment has no Apple SDK or macOS runtime, so a full native macOS build and run remain unverified. Both macOS architectures' Go terminal code cross-compiles.
- At 80×24 the command view retains core telemetry with a smaller clock. Below 60×20, a minimal clock/status/resize message replaces the full display. Very short diagnostics can omit rows. Processor arrays page when necessary. Canvas allocation is capped at 400×160.
- Unicode requires a font with block/quadrant/Braille coverage and normal terminal cell widths. External names preserve common Latin, Greek, and Cyrillic characters; combining marks are removed, and wide characters, emoji, control characters, and width-uncertain characters become `?`. Long names are clipped with `~`. `--ascii` is the safe fallback.
- Linux process resident memory uses the kernel's fast RSS estimate. Restricted process visibility can produce fewer than five entries. Containers can expose host CPU/memory totals alongside namespace-limited processes and interfaces.
- Resource figures reflect the test host and short profiles, not a cross-platform guarantee. A blocked filesystem collector can delay its telemetry, while input and clock rendering remain responsive. SIGKILL, a terminal emulator crash, and power loss cannot run cleanup; `stty sane` restores input after an uncatchable termination.

API references: [Linux CPU counters](https://www.man7.org/linux/man-pages/man5/proc_stat.5.html), [Linux process fields](https://www.man7.org/linux/man-pages/man5/proc_pid_stat.5.html), [Apple process information](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc_info.h), [Apple VM statistics](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/mach/vm_statistics.h), [Apple 64-bit interface messages](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/net/if.h).
