# Leaderboard — Fall 2026 LCARS Terminal System Monitor (`v3`)

**Final = Visual (human, /30) + Engineering (blind agent, /70) − Honesty deduction.** Engineering totals are the calibrated figures from `results/CALIBRATION.md`. Every scorecard was locked before `private/submission-map.tsv` and the visual scores were opened.

| Rank | Model | Anon ID | Language | Visual (/30) | Engineering (/70) | Honesty | **Final (/100)** |
| --: | :-- | :-- | :-- | --: | --: | --: | --: |
| 1 | Claude Opus 5.5 | S02 | Rust | 28.5 | 70.0 | 0 | **98.5** |
| 2 | Claude Fable 5.1 | S01 | Rust | 27.0 | 67.0 | −0.5 | **93.5** |
| 3= | GPT-6.1 Sol | S03 | Go | 24.0 | 67.5 | 0 | **91.5** |
| 3= | GPT-6 Astra | S07 | Go | 25.5 | 66.0 | 0 | **91.5** |
| 5 | Gemini 4 Argon | S09 | Rust | 28.5 | 63.0 | −1.0 | **90.5** |
| 6 | DeepSeek V4.1 Flash 1017 | S08 | Rust | 22.5 | 63.5 | 0 | **86.0** |
| 7 | Grok 4.7 | S04 | Rust | 22.5 | 63.0 | 0 | **85.5** |
| 8 | Kimi K3.1 Pro | S06 | Rust | 21.0 | 62.25 | 0 | **83.25** |
| 9 | Muse Wonder | S05 | Go | 19.5 | 55.0 | −1.0 | **73.5** |

GPT-6.1 Sol and GPT-6 Astra tie on 91.5. Ranked by engineering score instead, Sol (67.5) leads Astra (66.0); by visual score, Astra (25.5) leads Sol (24.0). The tie is left standing.

> **Conflict-of-interest disclosure.** The orchestrating evaluator and all nine blind grading subagents ran on **Claude Opus 5.5**, which is also a contestant (S02, ranked #1), and Claude Fable 5.1 (S01, #2) is from the same lab. Grading was anonymized: shuffled IDs, no model or vendor strings anywhere in the snapshots (verified by grep), and the map stayed sealed until every scorecard was locked. Even so, a model might recognize its own coding style, and the reader should weigh that. Every S02 point traces to reproducible probe output or a `file:line` citation in `results/S02.md`. The objective measurements (build, lint, tests, PTY lifecycle, perf, native macOS run) do not depend on the grader.

## Category breakdown (engineering, calibrated)

| Model | Arch & Scope /14 | Metrics & macOS /14 | Lifecycle & Robust /14 | Build & Tests /10 | Layout & Color /8 | Perf /6 | Docs /4 |
| :-- | --: | --: | --: | --: | --: | --: | --: |
| Claude Opus 5.5 | 14.0 | 14.0 | 14.0 | 10.0 | 8.0 | 6.0 | 4.0 |
| Claude Fable 5.1 | 13.5 | 12.5 | 14.0 | 10.0 | 7.0 | 6.0 | 4.0 |
| GPT-6.1 Sol | 13.5 | 12.5 | 14.0 | 9.5 | 8.0 | 6.0 | 4.0 |
| GPT-6 Astra | 12.5 | 12.5 | 13.5 | 9.5 | 8.0 | 6.0 | 4.0 |
| Gemini 4 Argon | 12.5 | 12.0 | 13.0 | 7.5 | 8.0 | 6.0 | 4.0 |
| DeepSeek V4.1 Flash 1017 | 12.5 | 10.5 | 13.5 | 9.5 | 7.5 | 6.0 | 4.0 |
| Grok 4.7 | 12.5 | 10.0 | 14.0 | 9.5 | 8.0 | 5.0 | 4.0 |
| Kimi K3.1 Pro | 13.0 | 10.5 | 13.25 | 9.0 | 7.25 | 5.5 | 3.75 |
| Muse Wonder | 11.0 | 8.5 | 12.0 | 9.0 | 6.0 | 5.0 | 3.5 |

## Decisive findings per model

**1. Claude Opus 5.5 (S02) — 98.5.** Perfect engineering score. It is the only submission whose macOS collector was **fully correct on native hardware** (`host_processor_info`, `host_statistics64`, `NET_RT_IFLIST2`, `proc_pidinfo` with `mach_timebase_info` scaling, `statvfs` on the Data volume): load, memory, disk, process count (917) and the top-5 all matched the reference. The sampler is pure, with injected time; it handles wrap vs reset, interface churn and PID reuse via `(pid, start)` keys. There are 77 deterministic tests, including a layout sweep from 44×14 to 260×90. Dependencies are `libc` + `unicode-width` only. Restoration is clean on every path, through a panic hook, a Drop guard and a signal self-pipe.

**2. Claude Fable 5.1 (S01) — 93.5.** Linux metrics, lifecycle, tests (63) and perf are all clean: 0.0% idle CPU, 2.9 MiB, about 1.1 KB/s of diffed output. On macOS, process CPU% is about **40× low** because `pti_total_*` Mach ticks are treated as nanoseconds (`macos.rs:392`). The compact tier corrupts at 60–77 columns. Honesty −0.5: the headline "well under a millisecond after `exec`" is timed from the top of `main`; the README discloses this further down, and the clean-room exec→first frame is 4.5 ms.

**3=. GPT-6.1 Sol (S03) — 91.5.** This has the highest engineering score of the Go entries (67.5) in the smallest codebase (1,917 LOC, zero third-party modules). Faults are isolated through an `Issues` list → `--` values plus a PARTIAL status, and it has tested rate edge cases including NaN/Inf dt. It renders at about 450 B/s. It has the same Apple Silicon Mach-tick CPU% bug as Fable and Astra (`collector_darwin.go:83,167`).

**3=. GPT-6 Astra (S07) — 91.5.** All 9 primary and 5 diagnostic metrics are exact on Linux. It is a true cell diff with the lowest bytes per frame of the nine (119 B). Startup is 3.9 ms. It has the Mach-tick CPU% bug on macOS (`collect_darwin.go:133`). The sampler goroutine lacks a `recover`, so a collector panic would bypass terminal restoration. CJK names are replaced with `?` rather than clipped by width. The macOS collector is cgo-only, so the cgo-free darwin cross-build fails (`metrics.NewCollector` undefined), as its README documents.

**5. Gemini 4 Argon (S09) — 90.5.** It ties for the top visual score (9.5) but drops 7 engineering points plus 1 for honesty. **`cargo fmt --check` fails** (35 hunks). Deprecated libc Mach calls break darwin clippy `-D warnings` and produce warnings in the native macOS build. On macOS, `proc_listallpids` is treated as bytes rather than a count, so it shows 297 of 915 processes and the top-5 is incomplete. Name clipping counts chars, not display width. Honesty −1: a 0.37 ms in-process microbenchmark is presented as whole-program startup, against a measured 30 ms first frame. Its Linux metrics, lifecycle and DECSET-2026 cell diffing are excellent.

**6. DeepSeek V4.1 Flash 1017 (S08) — 86.0.** Lifecycle and perf are flawless, and the build, lint and tests (38) are clean on both OSes. Its sysinfo-based process collector **counts threads as processes**, showing 1,626 vs about 506 on Linux, with duplicate thread IDs in the top-5. Its own statvfs code uses `max(f_frsize, f_bsize)`, so macOS disk shows **115 T for a 460 GiB volume**. It hardcodes "PRESSURE NOMINAL"-style status labels.

**7. Grok 4.7 (S04) — 85.5.** It has the leanest binary (468 KB, `libc` only), 0.0% CPU and 2.7 MiB. Its diffing is **row-level**: changed rows are rewritten whole at 11.4 KB/s, which scored 0.5/1.5. The diagnostic per-core pills always render "…" and the swap readout is clipped. On macOS it has both the `proc_listallpids` count bug (74 of 916 processes) and the unscaled Mach-tick CPU, so every top-5 row reads 0%.

**8. Kimi K3.1 Pro (S06) — 83.25.** It has the slowest startup (94 ms, still inside the 250 ms target) and the highest RSS (10.6 MiB), from sysinfo's initial scan. Like DeepSeek it counts threads as processes (1,630 vs 505) and shows a 931% CPU spike on the first frame after switching views. macOS memory available is wrong (19.8G "free" against about 10 GiB real). `NO_COLOR` strips the background-colour rails, so it differs from `--mono`, which keeps them. No tests cover sampling because the collector has no injectable source.

**9. Muse Wonder (S05) — 73.5.** Terminal lifecycle and non-TTY handling are excellent, but the **macOS backend is largely broken**: CPU, load, memory, per-core and swap show N/A on native hardware. The causes are `kern.cp_time` (which does not exist on macOS), a wrong loadavg `fscale` offset and wrong `if_data` byte offsets. The Linux primary view omits memory *available*, a required metric. The frame breaks at 40x10. There is no 16-colour path, and idle CPU is 1.5%. Honesty −1: "Verified the full brief" despite the omitted metric.

See `FINAL-REPORT.md` for the cross-cutting synthesis, `METHODOLOGY.md` for hosts and procedure, `COST-APPENDIX.md` for (non-scoring) cost data, and `results/SXX.md` for full evidence.
