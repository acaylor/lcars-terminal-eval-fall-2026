# Captain's Log — lcars

Stardate 2026-10-27 (see README.md "Captain's log" for the full entry,
mirrored here as the project documentation record asked for in the brief).

Built `lcars`, a Go terminal monitor whose default screen is a genuine
LCARS composition rather than a recolored dashboard: segmented left rail
with rotated labels, elbowed header band, giant half-block clock, sweeping
salmon gauge band, Braille history trace, and a starfield void. Diagnostics
(per-core, swap, top-5 processes) lives behind `p`/`Tab` and is the only
table in the app.

Verified on Linux: `/proc`-based metrics with deterministic fixture tests,
wall-clock rate math (counter resets → N/A), terminal-independent
rendering, diffed ANSI output, resize/signal handling with deferred
terminal restore, 80×24 compact mode, ASCII/mono/256/truecolor themes,
Darwin arm64 cross-compile, and measured startup ≈27 ms (--once median,
n=15), idle CPU 0.00–0.25 % of one core, RSS ≈7.7 MB, binary 1.9 MB
(stripped). Captures at `captures/primary-120x36.txt` and
`captures/diag-120x36.txt` pass the brief's 7-question visual acceptance
check (see README).

Known uncertainty, recorded honestly: the Darwin collectors are
type-checked and cross-compiled but unexecuted on macOS in this
environment; they are written to degrade to N/A per-metric rather than
fail. macOS shows memory total-only (cgo-free tradeoff).
