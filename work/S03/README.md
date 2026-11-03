# LCARS / Engineering

A sparse, asymmetric terminal command display for Linux and macOS. The clock is
the focal readout; broad elbows and segmented rails organize load, memory, root
filesystem usage and network traffic. Diagnostics are a separate view.

![Primary command view, 120×36](captures/primary.png)

## Engineering decision

Go provides quick startup, a small resident working set, native executables and
explicit cleanup with `defer`. A bounded cell grid and ANSI cell differences keep
refresh output low. There are **no third-party Go libraries**, downloads at
runtime, subprocesses for telemetry, or privileged operations.

macOS uses a small cgo bridge to the OS's Mach, sysctl, libproc and networking
APIs. These are system libraries; Linux requires no C bridge. Native API access
keeps the refresh loop independent of command-line utility formats.

## Build and run

Requires Go 1.22 or newer. Verification used **Go 1.27.1** on Linux amd64.
macOS also needs Apple's Command Line Tools and **`CGO_ENABLED=1`**. Race tests
need a C compiler on both systems. Install the toolchain before building; all
project code builds offline.

```sh
GOTOOLCHAIN=local GOPROXY=off make build
./bin/lcars
```

On macOS:

```sh
CGO_ENABLED=1 GOTOOLCHAIN=local GOPROXY=off make build
./bin/lcars
```

`make build` expands to this release command:

```sh
go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o bin/lcars ./cmd/lcars
```

Build natively on each OS/architecture. The same source and Go toolchain version
produce reproducible Linux releases; macOS additionally requires the same SDK
and C compiler. VCS stamping and absolute source paths are disabled. There are
no module dependencies to resolve. The binary can be copied to any directory on
your PATH. No installation service or configuration file is needed.

```sh
./bin/lcars --color mono
./bin/lcars --ascii --color basic
./bin/lcars --interval 500ms
./bin/lcars --snapshot --width 80 --height 24
./bin/lcars --snapshot --diagnostic
./bin/lcars --help
```

True color is detected through `COLORTERM=truecolor/24bit` or a `TERM` with
`direct/truecolor`. Otherwise `TERM=*256color*` selects indexed color, and other
VT terminals use ANSI colors. `NO_COLOR` selects monochrome. `--color` explicitly
overrides detection (`auto`, `true`, `256`, `basic`, `mono`). `--ascii` replaces
all non-ASCII display glyphs, including system text. Redirected input/output and
`TERM=dumb` are rejected with instructions to use `--snapshot`.

## Controls

| Key | Action |
| --- | --- |
| `q`, `Esc` | Quit and restore the terminal |
| `?` | Toggle help |
| Space | Pause/resume counters, clock and animation |
| `p` | Toggle process diagnostics |
| `Tab` | Switch command/diagnostic view |
| `Ctrl-C` | Exit through signal cleanup |

The command rail includes these hints. The default interval is one second;
`--interval` accepts 500 ms through 10 seconds. At 80×24 the clock keeps its
hierarchy while the telemetry becomes compact. Below 78 columns or 24 rows,
the app switches to a clipped emergency display. Resizing restores the richer
composition automatically. Canvas allocation is capped at 500×200 cells.

## Architecture and metric definitions

| Package | Responsibility |
| --- | --- |
| `internal/metrics` | Platform collectors, unavailable values, interval deltas and top five selection |
| `internal/ui` | Pure cell-grid rendering, adaptive composition, sanitized text, SVG capture |
| `internal/terminal` | Input mode, terminal size, capability detection and ANSI cell differences |
| `internal/app` | Controls, sampling schedule, signal lifecycle and offline captures |
| `cmd/lcars` | CLI validation and actionable errors |

The collector interface returns counters; the sampler computes rates using the
actual monotonic elapsed time. A first sample, reset counter or nonpositive
interval shows `--`. Matching network interfaces individually prevents hotplug
from creating artificial spikes. PID and process birth identity prevent reused
PIDs from inheriting another process's CPU history. Baselines own their buffers.
Process sampling runs only while diagnostics are open. History is bounded to
160 values; the displayed process list is bounded to five entries.

Linux reads `/proc/stat`, `/proc/meminfo`, `/proc/uptime`, `/proc/loadavg`,
`/proc/net/dev` and `/proc/<pid>/stat`. Process CPU tick frequency comes from
kernel `AT_CLKTCK` in `/proc/self/auxv`, rather than assuming 100 Hz. Filesystem
usage uses `statfs`. CPU idle includes iowait; guest time is already included in
user/nice and is not counted twice. Memory used is `MemTotal - MemAvailable`.

macOS uses `host_processor_info` for CPU counters, `host_statistics64` and
`hw.memsize` for memory, `kern.boottime` for uptime, `getloadavg`, `vm.swapusage`,
`statfs`, `getifaddrs`, and `proc_listallpids`/`proc_pid_rusage`/`proc_name`.
Mach allocations and host ports are released each sample. Available memory is
an estimate of **free + inactive pages**, and used memory is total minus that
estimate. Speculative pages are already included in free pages; see
[Apple's XNU accounting](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/host.c).
Process counters follow
[Apple's rusage structures](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/resource.h).

Overall CPU is a percentage of aggregate CPU capacity. Process CPU uses the
convention **100% = one core**, and may exceed 100%. Process memory is resident
bytes. Process count includes enumerated processes even when some individual
metrics cannot be read. Processes that disappear or deny access are skipped.
Disk used is `(blocks - free blocks) × block size`; total is all filesystem
blocks. Network rates sum all exposed non-loopback interface counters, including
virtual interfaces. No exposed external interfaces yields a measured zero after
the baseline. One failed collector leaves the others operating and marks the
display `PARTIAL`.

## Verification and captures

```sh
make test                          # go test -race ./...
make lint                          # go vet ./... and gofmt check
make captures                      # two real samples, one second apart
make perf                          # PTY startup, CPU, RSS, size and lifecycle
go test ./internal/ui -run '^$' -fuzz FuzzRender -fuzztime=3s -parallel=2
```

`scripts/measure.py` needs only Python's standard library and runs without a
live terminal. It measures time to the first complete ANSI frame in ten PTY
runs, executable bytes, child user/system CPU and live Linux RSS over six idle
seconds. macOS reports the child peak RSS across runs. It also exercises input,
signals, pause, resize, degraded rendering and redirected output. CPU includes
startup/exit overhead, and is reported against one core. This is a short local
measurement, not a claim about every machine.

Captures contain **real telemetry** from the verification environment. SVG and
text exports require no extra tools. Optional PNG conversion uses installed
`librsvg`/`libcairo` solely for documentation:

```sh
python3 scripts/rasterize.py
```

- [Primary SVG](captures/primary.svg) · [Primary text](captures/primary.txt)
- [Secondary SVG](captures/secondary.svg) · [Secondary text](captures/secondary.txt)
- [Exact commands and results](VERIFICATION.md)
- [Performance JSON](captures/performance.json)
- [Captain's log](CAPTAINS-LOG.md)

![Secondary diagnostics, 120×36](captures/secondary.png)

## Known limitations

- Linux was built and exercised here. Darwin's Go/terminal layers were cross
  compiled for arm64 and amd64. This Linux environment has no Apple SDK or Mac,
  so the native macOS cgo collector still needs a native build and runtime test.
  `CGO_ENABLED=0` Darwin builds expose unavailable metrics and are not releases.
- macOS available memory is an estimate, rather than Linux `MemAvailable` or
  Activity Monitor's memory pressure model. Its legacy interface counters may
  roll over; that interval becomes unavailable and the following one recovers.
- Small diagnostic views show only the cores that fit, plus the omitted count.
  Enlarge the terminal to see more. The emergency display prioritizes essentials.
- Unreadable processes cannot be ranked against processes with readable CPU
  counters. Newly opened diagnostics need another sample for process rates.
- Unicode requires a font with single-cell block and Braille glyphs. Combining
  marks are omitted and wide/emoji characters become `?` to preserve alignment.
  `--ascii` provides the explicit fallback. Captures use DejaVu Sans Mono.
- Container metrics follow exposed kernel/filesystem data and PID/network
  namespaces; they do not calculate cgroup quotas. This capture environment
  exposed four processes and no external network interfaces.
- `SIGKILL`, terminal emulator failure and power loss cannot run cleanup code.
  Conventional job-control suspension is not supported; quit before suspending.
