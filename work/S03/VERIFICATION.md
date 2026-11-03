# Verification record

Verified on 2026-09-30, Linux 6.8.0-142-generic, amd64, Go 1.27.1.
The workspace has a read-only home cache, so commands used
`GOCACHE=/tmp/lcars-go-cache`. Toolchain-manager cache warnings were environmental;
the application's build, tests and measurements completed successfully.

## Build, tests and lint

Exact commands used:

```sh
gofmt -w internal
GOCACHE=/tmp/lcars-go-cache make build test lint captures
python3 scripts/rasterize.py
python3 scripts/measure.py ./bin/lcars > captures/performance.json
GOCACHE=/tmp/lcars-go-cache go test ./internal/ui -run '^$' -fuzz FuzzRender -fuzztime=3s -parallel=2
file bin/lcars
GOCACHE=/tmp/lcars-go-cache go test -cover ./...
```

| Check | Result |
| --- | --- |
| Release build | PASS; stripped, statically linked Linux amd64 ELF |
| `go test -race ./...` | PASS; metrics, UI, terminal and app packages |
| `go vet ./...` | PASS |
| `test -z "$(gofmt -l cmd internal)"` | PASS |
| Rendering fuzz run | PASS; 15,362 executions, two workers, no crashes |
| Live Linux collector | PASS; uptime, memory, CPU and process metrics |
| 120×36 capture export | PASS; real counters sampled one second apart |
| SVG → PNG | PASS; installed librsvg/libcairo, DejaVu Sans Mono |
| PTY interactions and cleanup | PASS; full details below |
| Go coverage run | PASS; metrics 94.6%, UI 91.2%, terminal 50.5%, app 13.5%, CLI 0% |

Automated tests cover elapsed-time rates, zero/negative/nonfinite intervals,
long sample gaps, counter resets, interface appearance/disappearance, PID reuse,
process disappearance, bounded top five selection and history, collector failure
isolation, Linux parsers, ANSI differences, color fallbacks, text sanitization,
SVG validity, help/pause/view controls and layout sizes from 1×1 through 500×200.

The PTY harness exercises `q`, Esc, actual terminal-generated Ctrl-C, SIGINT,
SIGTERM, SIGHUP and SIGQUIT; it compares terminal settings before/after and checks
cursor, wrapping and alternate-screen restoration sequences. It also checks
help, `p`, Tab, paused output silence, resize through 80×24, 20×5, 1×1 and 120×36,
ASCII/monochrome, redirected output rejection and a plain snapshot. Steady
refreshes emit no screen-clear sequence.

The PTY harness exercises the separately built executable, so its integration
coverage is not included in the Go coverage percentages.

Early checks exposed a read-only VCS stamping error, reused collector-buffer
baselines, an environment-dependent color test, a test import collision and an
unkeyed test struct literal flagged by vet. All were corrected and the checks
rerun successfully. The first Ctrl-C harness lacked a controlling terminal;
attaching the child to its PTY fixed the harness and verified real signal cleanup.

## Performance

Exact measurement command:

```sh
python3 scripts/measure.py ./bin/lcars > captures/performance.json
```

Results from [the recorded JSON](captures/performance.json):

| Measurement | Observed |
| --- | ---: |
| Startup median, 10 runs | 3.032 ms |
| Startup maximum | 3.318 ms |
| Executable size | 2,015,356 bytes (1.92 MiB) |
| Idle sample duration | 6.000 s |
| Child user + system CPU, including startup/exit | 0.006531 s |
| CPU, relative to one core | 0.109% |
| Live resident memory | 4,206,592 bytes (4.01 MiB) |
| First complete frame | 9,426 output bytes |
| Subsequent output over six idle seconds | 607 bytes |

Startup includes process launch and the first complete cell frame. CPU is child
`getrusage` divided by measured elapsed time. RSS comes from `/proc/<pid>/status`.
Numbers describe this short Linux primary-view run; diagnostics scale with the
number of processes and cores. They are not extrapolated to untested systems.

## Reproducibility and Darwin compilation

```sh
GOCACHE=/tmp/lcars-go-cache GOTOOLCHAIN=local GOPROXY=off make build
sha256sum bin/lcars
GOCACHE=/tmp/lcars-go-cache GOTOOLCHAIN=local GOPROXY=off go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o /tmp/lcars-repro ./cmd/lcars
sha256sum /tmp/lcars-repro
cmp bin/lcars /tmp/lcars-repro
GOCACHE=/tmp/lcars-go-cache GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build -buildvcs=false -o /tmp/lcars-darwin-arm64 ./cmd/lcars
GOCACHE=/tmp/lcars-go-cache GOOS=darwin GOARCH=amd64 CGO_ENABLED=0 go build -buildvcs=false -o /tmp/lcars-darwin-amd64 ./cmd/lcars
```

Offline Linux builds passed and matched byte for byte:

```text
ad4420588843faa4f68d41a917f98a81811f8780e8fad934e2e6cb4755777257
```

Darwin arm64 and amd64 Go/terminal compilation passed. These builds intentionally
exclude the native collector and are **not macOS release verification**. An Apple
SDK is unavailable here. Native cgo compilation and macOS runtime behavior remain
unverified. To complete that check on a Mac, run:

```sh
CGO_ENABLED=1 make build test lint captures perf
```

## Visual acceptance at 120×36

Both PNGs were inspected after rendering their actual cell grids; SVG and text
counterparts are included. They contain measured data, not fixture metrics.

| Acceptance question | Assessment |
| --- | --- |
| Asymmetric silhouette? | Yes; segmented port rail, swept upper elbow, offset transverse rail and lower return |
| One dominant focal element? | Yes; the large amber local clock |
| At least 20% intentional negative space? | Yes; 3,133 of 4,320 cells (72.52%) have blank text and the base background |
| Metrics integrated into the composition? | Yes; clock, status capsule, large load, memory labels and root aperture occupy distinct frame regions |
| Recognizable with text removed? | Yes; elbow, colored rail segments, transverse sweep and lower return retain the structure |
| Materially different from top/htop/btop? | Yes; the default screen is a command composition with no process table or dashboard boxes |
| Process list absent by default? | Yes; it appears only in diagnostics, tested independently |

The sandbox's PID namespace exposed four processes at capture time, so the
secondary view correctly lists four. Its network namespace exposed only loopback,
so non-loopback receive/transmit sums are zero after the second sample. Memory,
CPU and load reflect the kernel data made available to the application.

![Primary view](captures/primary.png)

![Secondary view](captures/secondary.png)
