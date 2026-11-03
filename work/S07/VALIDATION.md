# Verification record

Verified on 2026-09-06 in the supplied Linux sandbox: Ubuntu 24.04.4 LTS, Linux 6.8.0-139-generic, amd64, AMD Ryzen 9 7900X (24 logical cores), Go 1.27.1, Python 3.12.3. Final build, tests, lint, PTY checks, and capture generation passed. The runtime has no third-party dependencies.

## Exact commands executed

The environment requires writable compiler/tool caches. These prefixes were used on Go/Make commands:

```sh
env GOCACHE=/tmp/lcars-go-cache MISE_CACHE_DIR=/tmp/lcars-mise make build test lint captures
```

The Make targets executed:

```sh
go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o bin/lcars ./cmd/lcars
go test -race ./...
go vet ./...
test -z "$(gofmt -l cmd internal)"
go run -buildvcs=false ./cmd/capture -out artifacts
```

Build passed: a stripped, statically linked Linux amd64 ELF, **2,105,468 bytes (2.01 MiB)**. All **16 unit tests**, their subtests, and fuzz seed tests passed under the race detector. `go vet` and formatting checks passed. Tests cover interval units, zero/long/backward intervals, resets/rollovers, interface churn, PID reuse, disappearing processes, ranking, kernel parsers, collector failure isolation, native Linux sampling, 120×36 composition, all five rows at 80×24, core pages, very small/large sizes, hostile names, ASCII, color modes, unchanged/delta rendering, help, pause, and bounded history.

Capture rasterization (Pillow 192.0.2, optional development tool only):

```sh
/usr/bin/python3 scripts/render_captures.py artifacts/primary-120x36.svg artifacts/secondary-120x36.svg artifacts/compact-80x24.svg artifacts/ascii-80x24.svg
```

Passed; PNG, SVG, ANSI, and plain-text exports are in `artifacts/`. Captures use real OS samples, not fixtures. The PNG converter draws the exported terminal-cell geometry, including quadrant blocks and Braille dots, with DejaVu Sans Mono text. Metadata is in [capture-metadata.json](artifacts/capture-metadata.json).

```sh
/usr/bin/python3 scripts/verify_terminal.py --binary bin/lcars --seconds 12 --output artifacts/performance.json
```

Passed: q, Escape, Ctrl-C through a controlling PTY, SIGTERM, SIGHUP, SIGQUIT, exact termios restoration, cursor/alternate-screen restoration, help, p/Tab, pause/resume, ignored arrow sequences, ASCII/monochrome, redirected I/O, TERM=dumb, snapshot output, invalid flags, and resizes through 80×24, 60×20, 10×3, 1×1, 160×50, and 120×36. The output contains no screen-clear command. Pause emits no frame output after settling.

## Measured performance

The startup timer begins before launching the process and ends when the completed initial composition's footer is received through the PTY. It includes Python process-launch overhead and up to 5 ms of polling resolution. It measures interactive display availability, not the second sample required for meaningful CPU/rates.

| Measurement | Command view | Diagnostics |
| --- | ---: | ---: |
| Profile wall time | 12.000 s | 12.000 s |
| Total process CPU time, including startup/exit | 0.018186 s | 0.018806 s |
| CPU, 100% = one core | **0.152%** | **0.157%** |
| Peak live RSS sampled every 100 ms | **7.96 MiB** | **8.13 MiB** |
| Initial output | 6,908 bytes | 6,698 bytes |
| Subsequent output per second | **251.1 B/s** | **284.9 B/s** |

Ten startup trials: **5.327 ms median, 5.378 ms maximum**. Full raw measurements are in [performance.json](artifacts/performance.json). Live RSS is read from the running executable's `/proc/<pid>/status` on Linux; the script uses libproc on macOS. The 15 MiB `getrusage` lifetime peak includes the Python launcher's inherited footprint, so it is recorded separately and not presented as application RSS. These are short measurements on this host; the CPU test has only the sandbox's visible processes and no non-loopback interfaces.

```sh
env GOCACHE=/tmp/lcars-go-cache MISE_CACHE_DIR=/tmp/lcars-mise go test ./internal/ui -run '^$' -bench . -benchmem -count=1
```

Passed. `BenchmarkRender120x36`: **26,940 ns/op**, 59,851 B/op, 126 allocations. `BenchmarkUnchangedDiff`: **4,723 ns/op**, 32 B/op, one allocation. Unchanged frames produce zero terminal bytes. These benchmarks exclude OS collection.

```sh
env GOCACHE=/tmp/lcars-go-cache MISE_CACHE_DIR=/tmp/lcars-mise GOMAXPROCS=2 go test ./internal/metrics -run '^$' -fuzz FuzzProcessParser -fuzztime=2s
```

Passed: **298,420 executions in 2.06 seconds**, no parser panic or failure.

## Portability and reproducibility

```sh
env GOCACHE=/tmp/lcars-go-cache MISE_CACHE_DIR=/tmp/lcars-mise CGO_ENABLED=0 GOOS=linux GOARCH=arm64 go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o bin/lcars-linux-arm64 ./cmd/lcars
env GOCACHE=/tmp/lcars-go-cache MISE_CACHE_DIR=/tmp/lcars-mise GOOS=darwin GOARCH=arm64 CGO_ENABLED=0 go build ./internal/terminal
env GOCACHE=/tmp/lcars-go-cache MISE_CACHE_DIR=/tmp/lcars-mise GOOS=darwin GOARCH=amd64 CGO_ENABLED=0 go build ./internal/terminal
```

All passed. Linux arm64 was compiled, not executed. The Darwin commands verify **only the Go terminal package** on both architectures. The native macOS collector and full application require cgo plus Apple's SDK and have **not** been built or run in this Linux environment. Their API choices were checked against Apple's published XNU headers; that is not a substitute for native testing.

```sh
env GOCACHE=/tmp/lcars-go-cache MISE_CACHE_DIR=/tmp/lcars-mise go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o /tmp/lcars-rebuild ./cmd/lcars
sha256sum bin/lcars /tmp/lcars-rebuild
file bin/lcars
```

Both release builds produced the same SHA-256:

```text
f6a0a442e44f0737f938307494d635f7b9b1ada821c81f70658e9f3a68cfcb6c
```

This verifies reproducibility for the same local toolchain and target, not between Go or Apple SDK versions.

## Visual acceptance, 120×36

The final [primary capture](artifacts/primary-120x36.png) was opened and inspected, along with [diagnostics](artifacts/secondary-120x36.png), the compact view, and the ASCII fallback.

| Acceptance question | Assessment |
| --- | --- |
| Asymmetric silhouette? | Yes: a thick left spine and sweeping upper/lower returns, with an open right side. |
| One dominant focal element? | Yes: the wide amber clock. |
| At least 20% intentional negative space? | Yes: **70.09%** of cells are untouched background, measured from the final grid. |
| Integrated metrics instead of interchangeable boxes? | Yes: the load, CPU, memory, disk, and network occupy an open composition without widget borders. |
| Recognizable without text? | Yes: the stepped curves, segmented spine, and long return rails establish the silhouette. |
| Materially different from top/htop/btop? | Yes: clock-centered composition, broad rail geometry, open space, no primary table. |
| Processes absent by default? | Yes: the primary renderer never draws them; tests and PTY checks enforce this. |

The first review prompted curved quadrant elbows and a correction to Go's date layout (uppercase weekday/month text had been interpreted as literals). The final capture uses the actual local date. The initial build failed on this workspace's incomplete VCS metadata, an initial test build had an import/function name collision, and an initial vet run found unkeyed test literals; all were corrected before the passing final checks. There are no outstanding Linux check failures.
