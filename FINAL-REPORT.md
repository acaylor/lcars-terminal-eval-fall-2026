# Final Report — Fall 2026 LCARS Terminal System Monitor (`v3`)

*Engineering evaluation of nine Fall 2026 frontier coding models on `prompt-v3`. Written to drop into the draft post `lcars-terminal-fall-2026-evaluation.md`. Hosts, scripts and protocol are in `METHODOLOGY.md`, ranked results in `LEADERBOARD.md`, and per-submission evidence in `results/SXX.md`.*

## Results

| Rank | Model | Lang | Visual /30 | Eng /70 | Honesty | **Final** |
| --: | :-- | :-- | --: | --: | --: | --: |
| 1 | Claude Opus 5.5 | Rust | 28.5 | 70.0 | 0 | **98.5** |
| 2 | Claude Fable 5.1 | Rust | 27.0 | 67.0 | −0.5 | **93.5** |
| 3= | GPT-6.1 Sol | Go | 24.0 | 67.5 | 0 | **91.5** |
| 3= | GPT-6 Astra | Go | 25.5 | 66.0 | 0 | **91.5** |
| 5 | Gemini 4 Argon | Rust | 28.5 | 63.0 | −1.0 | **90.5** |
| 6 | DeepSeek V4.1 Flash 1017 | Rust | 22.5 | 63.5 | 0 | **86.0** |
| 7 | Grok 4.7 | Rust | 22.5 | 63.0 | 0 | **85.5** |
| 8 | Kimi K3.1 Pro | Rust | 21.0 | 62.25 | 0 | **83.25** |
| 9 | Muse Wonder | Go | 19.5 | 55.0 | −1.0 | **73.5** |

> **Disclosure.** I ran this evaluation, and the nine blind graders, on Claude Opus 5.5, which is also the winning contestant. The safeguards were anonymized snapshots with no identity strings, sealed mapping and visual scores, and scorecards locked and then calibrated while still blind. The hard numbers (build, lint, tests, PTY lifecycle, perf, native macOS runs) come from scripts, not judgement. Treat the judgement-based items accordingly; the S02 scorecard cites `file:line` for every point.

## The headline: this round was decided on macOS

The field was much tighter than in the summer round. **All nine compiled from a clean snapshot with zero Linux warnings and passed their own test suites. All nine restored the terminal on every exit path. None shelled out to `top`/`ps`/`vm_stat`. All nine handled redirected output with an actionable message.** Engineering scores therefore separated almost entirely on the parts that are hard to get right without running the code: the macOS collector, and smaller correctness details in the secondary view.

Eight of nine READMEs said honestly that the author never ran the macOS build. Gemini's makes no statement either way. We did run it, natively, on an M4 Mac (macOS 27.2). **Only one of nine macOS collectors was fully correct on real hardware.**

### macOS collector audit (native run vs `sysctl`/`vm_stat`/`top`/`df` reference)

| Model | Approach | What broke on real hardware | Score /5 |
| :-- | :-- | :-- | --: |
| Claude Opus 5.5 | Hand-written Mach/sysctl/libproc FFI | Nothing. It scales `proc_pidinfo` task times by `mach_timebase_info` correctly (only Gemini also does this) | 5.0 |
| Claude Fable 5.1 | Hand-written FFI | Process CPU% about 40× low (Mach ticks read as ns) | 3.5 |
| GPT-6.1 Sol | cgo | Same Mach-tick bug (about 42× low) | 3.5 |
| GPT-6 Astra | cgo | Same Mach-tick bug (about 40× low) | 3.5 |
| Kimi K3.1 Pro | `sysinfo` | Memory available about 2× high (used + free > total) | 3.5 |
| DeepSeek V4.1 Flash | `sysinfo` + own statvfs | Disk 256× too large (`max(f_frsize, f_bsize)`; Darwin `f_bsize` = 1 MiB) | 3.5 |
| Gemini 4 Argon | Hand-written FFI | `proc_listallpids` count treated as bytes, so 297 of 915 processes and an incomplete top-5; deprecated libc Mach symbols | 3.0 |
| Grok 4.7 | Hand-written FFI | The same `proc_listallpids` bug (74 of 916) **plus** the Mach-tick bug, so every top-5 row reads 0% | 2.5 |
| Muse Wonder | Pure-Go syscalls with hand-computed offsets | CPU, load, memory, per-core and swap all N/A. `kern.cp_time` does not exist on macOS, the loadavg and `if_data` offsets are wrong, and TX reads the error counter | 1.0 |

Two bugs account for most of the losses. **Four of the seven native (non-`sysinfo`) collectors** treated `proc_taskinfo.pti_total_user/system` as nanoseconds. That is true on Intel Macs, but on Apple Silicon the values are Mach absolute-time ticks (24 MHz timebase), so they must be scaled by `mach_timebase_info`. **Two** misread the `proc_listallpids(NULL, 0)` return value, which is a PID count, as a byte count. Neither bug shows up in a cross-compile, `go vet`, clippy, or a code review that trusts the man page. It only appears when you run the binary on Apple Silicon.

## Visual vs engineering disagreements

- **Gemini 4 Argon: top visual, mid engineering.** It tied Opus for the best visual score (9.5) but scored 63/70. It is the only submission that fails `cargo fmt --check`. It also fails darwin-target clippy (deprecated libc Mach calls), has the macOS PID-buffer bug, and gets an honesty deduction for presenting a 0.37 ms in-process microbenchmark as startup time (clean-room first frame: 30 ms). The same model produced the most striking screen and the least polished toolchain hygiene.
- **GPT-6.1 Sol: modest visual, near-top engineering.** It scored 8.0 visually but 67.5 on engineering, the highest outside Opus. It did this in the smallest codebase of the nine (1,917 LOC, zero dependencies), with excellent fault isolation and the lowest steady output of any entry (about 450 B/s).
- **Agreement at both ends.** Opus led both axes, and Muse Wonder was last on both. Across the middle the axes are weakly correlated. Kimi (7.0 visual) and DeepSeek and Grok (7.5) all scored within 1.25 engineering points of Gemini (9.5).

## Rust vs Go

| | Rust (6) | Go (3) |
| :-- | :-- | :-- |
| Mean engineering | 64.8 | 62.8 (66.75 without Muse) |
| Binary size | 0.47–0.82 MB | 1.97–2.11 MB |
| Peak idle RSS | 2.7–10.6 MiB (median 3.6) | 5.1–8.7 MiB |
| Startup (first byte) | 2.5–94 ms | 3.2–3.7 ms |
| macOS strategy | 4 hand-written FFI, 2 `sysinfo` | 2 cgo, 1 pure syscall |

Language did not decide outcomes. The two OpenAI Go entries beat four of the six Rust entries on engineering. Go paid a fixed cost of about 2 MB of binary and a few MiB of GC heap; GPT-6.1 Sol's RSS drifted from 5.2 to 8.2 MiB over 60 s before levelling off. The two cgo collectors only build for darwin with `CGO_ENABLED=1`, which is documented, and GPT-6 Astra's cgo-free darwin cross-build fails outright. Among the Rust entries, the two that used `sysinfo` (Kimi, DeepSeek) were the slowest to start (94 ms and 39 ms against 2.5–3 ms for the others). On Linux, both inherited sysinfo's habit of listing **threads as processes**, reporting about 1,630 processes against roughly 505 real ones, with duplicate thread IDs in the top-5.

## Terminal lifecycle and signals

Every binary was killed five ways in a 120x36 PTY on Linux, with termios recorded before and after. The same was done natively on macOS.

| Path | Exited | termios restored | alt-screen exit | cursor shown |
| :-- | :-: | :-: | :-: | :-: |
| `q` | 9/9 | 9/9 | 9/9 | 9/9 |
| `Esc` | 9/9 | 9/9 | 9/9 | 9/9 |
| `Ctrl-C` byte | 9/9 | 9/9 | 9/9 | 9/9 |
| OS `SIGINT` | 9/9 | 9/9 | 9/9 | 9/9 |
| `SIGTERM` | 9/9 | 9/9 | 9/9 | 9/9 |

Kimi and DeepSeek emit no SGR reset *after* the quit signal, but their last frame already ends in `\x1b[0m`, so the terminal is effectively clean. DeepSeek also overwrites the window title with "Terminal" on exit. Gemini and Grok ship `panic = "abort"`, but both install a panic hook that restores the terminal first, so the RAII-guard pitfall is avoided. The one latent risk found is in GPT-6 Astra: its sampler goroutine has no `recover`, so a collector panic would bypass restoration.

Every binary survived 80x24, 40x10 and 20x5 and the live resize sequence 120x36 → 80x24 → 30x8 → 120x36 with no panics. Layout quality at small sizes did vary: Muse's footer overwrites content at 40x10, and Fable's compact tier collides between 60 and 77 columns.

## Performance (Linux, Ryzen 9 7900X, 120x36, median of 15 starts, 8 s idle window)

| Model | First byte | First frame | Idle CPU | Peak RSS | PTY bytes/s | Bytes/frame | Binary |
| :-- | --: | --: | --: | --: | --: | --: | --: |
| Claude Opus 5.5 | 2.7 ms | 3.2 ms | 0.13% | 3.6 MiB | 4,318 | 720 | 589 KB |
| Claude Fable 5.1 | 2.5 ms | 4.5 ms | 0.0% | 2.9 MiB | 1,096 | 274 | 671 KB |
| GPT-6.1 Sol | 3.2 ms | 4.2 ms | 0.0% | 5.0 MiB | 452 | 402 | 2.02 MB |
| GPT-6 Astra | 3.7 ms | 3.9 ms | 0.13% | 7.9 MiB | 492 | 119 | 2.11 MB |
| Gemini 4 Argon | 3.1 ms | 30.3 ms | 0.0% | 3.6 MiB | 1,634 | 1,189 | 557 KB |
| DeepSeek V4.1 Flash | 39.4 ms | 41.5 ms | 0.63% | 4.2 MiB | 3,691 | 509 | 824 KB |
| Grok 4.7 | 3.0 ms | 6.1 ms | 0.0% | 2.7 MiB | **11,446** | **11,452** | 468 KB |
| Kimi K3.1 Pro | 93.8 ms | 97.6 ms | 0.25% | 10.6 MiB | 618 | 619 | 721 KB |
| Muse Wonder | 3.3 ms | 20.3 ms | **1.50%** | 8.5 MiB | 612 | 612 | 1.97 MB |

All nine are well inside the 250 ms startup target. None issued a full-screen clear (`\x1b[2J` or RIS) on a steady-state refresh, and memory was flat or bounded over a further 60 s run. Output volume depends on how much each design animates. Opus and DeepSeek redraw animated rails 6–7 times a second, but each redraw is a cell diff. Grok is the outlier: it rewrites every changed *row* in full, sending a near-full screen of bytes each second. Muse is the only entry above 1% idle CPU. CPU resolution is 0.125% (100 Hz ticks over 8 s), so "0.0%" means under one tick.

## Honesty deductions

Only three were applied. Eight READMEs disclosed that macOS had not been run natively, and Gemini's made no claim either way. Test counts, lint status and perf numbers otherwise held up within the same order of magnitude.

| Model | Claim | Evidence | Deduction |
| :-- | :-- | :-- | --: |
| Gemini 4 Argon | README:12 "Measured cold startup + first-frame render is **~0.37 ms** (nearly 700× faster than the 250 ms target)"; CAPTAINS_LOG:22 likewise | An in-process `bench.rs` timing that excludes exec, terminal setup, a 25 ms warm-up sleep and the terminal write. Clean-room first frame: 30.25 ms | −1.0 |
| Muse Wonder | README Captain's log: "Verified the full brief" | The primary view never shows memory *available*, a required metric; per-core is truncated at 120x36; the frame breaks at 40x10 | −1.0 |
| Claude Fable 5.1 | README:17-18 "starts in well under a millisecond after `exec` (measured 0.4–0.5 ms…)" | Timed from the top of `main`. README:248 admits it "excludes `exec`", contradicting the headline. Clean-room exec→first frame: 4.5 ms | −0.5 |

Microbenchmarks presented as startup time were the most common way docs overstated performance. Two of the three deductions are for exactly that.

## Patterns worth noting for the post

1. **The Mach-tick trap.** Four models wrote the same wrong unit assumption. If you write native macOS process code, test it on Apple Silicon.
2. **`sysinfo` is not free.** It cost 40–90 ms of startup, and on Linux its process list includes threads unless you filter them.
3. **Zero TUI frameworks, nine diff renderers.** All nine hand-rolled a cell buffer. Eight of the nine diff at cell or span level; Grok diffs at row level.
4. **Toolchain hygiene was near-universal.** Eight of nine were fmt- and lint-clean on Linux and on the darwin target. The one exception (Gemini) failed `cargo fmt --check` and darwin clippy; its docs did not claim otherwise.
5. **Blind calibration mattered.** Nine independent graders produced two arithmetic errors and inconsistent macOS bands (a wrong memory figure scored 4.5 in one card while a wrong disk figure scored 3.5 in another). A blind cross-card pass fixed these before unblinding. The net effect was ±0.5–1.0 on four cards. Ranking effects: GPT-6 Astra (92.0 before) fell into a tie with GPT-6.1 Sol at 91.5, and DeepSeek (86.0) moved ahead of Grok (86.5 → 85.5). The top two and the bottom two were unchanged.

## Evaluator caveats
- The harness PNG/SVG renderer mis-mapped one pyte colour name (`bfightmagenta`), dropping bright-magenta backgrounds in the `color16-*` images. The raw streams and SGR statistics are correct, and graders verified against them. Scores are unaffected.
- The macOS host's toolchain lacked clippy, so Darwin linting ran from Linux against `aarch64-apple-darwin` (check only).
- One host per OS. Absolute performance numbers will differ on other hardware; the ratios between submissions should hold.
