# Engineering Grading Rubric (blind packet)

> Extracted verbatim from the evaluator brief: source-inspection checklist (§3.4) and scoring rubric (§4). Human visual scores are intentionally NOT included.

### 3.4 Deep Source-Code Inspection Checklist
Read every submission's source files (`src/`, `cmd/`, `internal/`, `tests/`) and record `file:line` citations for:
1. **Module boundaries & main-loop cleanliness:** Are metric collection, rate/state calculation, layout/canvas rendering, and terminal/event-loop handling separated cleanly, or tangled in a monolithic file/loop?
2. **OS collector isolation & macOS implementation:**
   - Are Linux and macOS isolated behind a clean trait/interface?
   - Inspect the `macos` / `darwin` code path line by line (or verify `sysinfo` configuration): does it collect real CPU, per-core CPU, loadavg, memory, swap, root disk, network RX/TX bytes, uptime, process count, and top-5 CPU processes via real APIs (`host_statistics64`, `host_processor_info`, `sysctl`, `proc_listallpids`/`proc_pidinfo`, `NET_RT_IFLIST2`/`getifaddrs`, `statvfs`/`statfs`), or are parts stubbed/hardcoded/broken?
   - **Forbidden subprocess check:** Search the entire codebase for `Command::new`, `exec.Command`, `popen`, `system(`, etc. Shelling out to `top`, `uptime`, `ps`, `vm_stat`, `df`, `ifconfig`, or `date` on refresh is an explicit `prompt-v3.md` violation.
3. **Rate math & robustness edge cases:**
   - Are network/CPU rates computed using **actual measured elapsed monotonic time** between samples rather than assuming `dt = 1.0s`?
   - How does the code handle counter rollover/decrease, newly appeared or vanished network interfaces, vanished PIDs mid-scan, zero/tiny or huge `dt`, permission-denied errors, long hostnames/process names, and multi-byte/CJK Unicode widths?
   - Does a single failed collector degrade that metric to an unavailable indicator, or abort the whole sample/display?
4. **Memory safety, `unsafe` hygiene & panic behavior:**
   - Audit every `unsafe` block (or `cgo` block): is it minimal, sound (e.g., buffer sizes, null-termination, initialized structs), and commented where non-obvious?
   - Check `Cargo.toml` `panic = "abort"` vs `panic = "unwind"` alongside terminal raw-mode guards: if a Rust binary sets `panic = "abort"`, RAII `Drop` guards do **not** run on panic unless a custom panic hook explicitly restores the terminal before aborting!
   - Confirm history ring buffers/vectors are bounded so long-running sessions cannot leak memory.
5. **Test quality & determinism:**
   - Read the actual test functions: do they deterministically test sampling/parsing, elapsed-time rate math (including counter resets/edge cases), layout tiers (`120x36`, `80x24`, tiny), and terminal-independent rendering—or are they trivial smoke assertions?
6. **Scope discipline & honesty cross-check:**
   - Check for unrequested bloat prohibited by `prompt-v3.md` (config files, mouse support, plugin systems, extra themes, extra rendering protocols like Kitty/Sixel, process killing/renicing).
   - Compare every factual claim in `README.md`, `CAPTAINS_LOG.md`, and `VERIFICATION.md`/`VALIDATION.md` (test counts, clean `fmt`/`clippy`, macOS verification claims, cell-diffing claims, measured startup/CPU/RSS numbers) against what the source code and your clean-room runs actually show.

---

## 4. Scoring Rubric (100 Points + Honesty Deductions)

- **Human Visual Score:** **30 points** (`1–10` × 3, added only after locking engineering scorecards).
- **Agent Engineering Score:** **70 points** across the seven categories below (0.5-point increments allowed).
- **Honesty Deductions:** **0 to −10 points**, subtracted after summing Visual + Engineering.
- **Build Gate:** A project that fails to compile from its clean snapshot scores **0** on all runtime-dependent items and is **capped at 30/100 total** (including visuals).

### Category 1: Code Architecture & Scope Discipline — 14 points
| Item | Band | Criteria |
| :--- | ---: | :--- |
| Module structure & separation of concerns | 0–4 | **4:** Clean, idiomatic separation of OS collection, rate/domain state, headless layout/canvas rendering, and terminal event loop; main loop is concise. **2:** Partial split, oversized mixed-responsibility modules. **0:** Monolithic structure. |
| OS collector interface isolation | 0–3 | **3:** Linux and macOS backends isolated behind a clean trait/interface with zero OS-specific leakage into UI or state logic. **1–2:** Leaky `#[cfg]` sprawl or ad-hoc branching. **0:** Single-OS or unisolated. |
| `unsafe` / FFI hygiene, panic safety & code completeness | 0–3 | **3:** `unsafe`/`cgo` minimal, sound, and explained; panic path still restores terminal (unwind or explicit panic hook before abort); bounded memory; zero TODOs/placeholders. Deduct for unsound FFI, unguarded `panic = "abort"` over RAII tty guards, unbounded buffers, or explanatory-comment bloat. |
| Dependency minimalism & justification | 0–2 | **2:** Minimal, well-chosen dependencies clearly justified in docs. **1:** Heavy/unnecessary transitive footprint or weak justification. **0:** Undocumented sprawl. |
| Scope discipline | 0–2 | **2:** Primary screen omits process list, and project avoids unrequested bloat (config files, mouse, themes, plugins, process-management actions). Deduct **1–2** for scope creep or **2** if process list appears on default view. |

### Category 2: System Metrics & Cross-Platform Collectors — 14 points
| Item | Band | Criteria |
| :--- | ---: | :--- |
| Primary view metrics & elapsed-time rate math (Linux verified) | 0–5 | All 9 primary metrics present and accurate against live host references (local time w/ seconds, date, hostname, uptime, overall CPU, load averages, memory used/avail/total, root disk, network RX/TX rates), with rates computed from **measured elapsed time** and zero subprocess shelling out. (Deduct **up to 3** if shelling out on refresh.) |
| Secondary diagnostic metrics (Linux verified) | 0–4 | All 5 secondary metrics present and accurate on `p` view (OS + arch, per-core CPU, swap when available, process count, top-5 CPU processes with PID, name, CPU%, memory). |
| macOS collector implementation (source audit + cross-check) | 0–5 | **5:** Complete, genuine macOS implementation covering all required primary + secondary metrics via native Darwin APIs (`sysctl`, Mach `host_statistics64`/`host_processor_info`, `libproc`, `NET_RT_IFLIST2`/`getifaddrs`, `statvfs`/`statfs`) or properly wired `sysinfo`, with zero subprocess shelling out and sound FFI types. **2–3:** Partial macOS coverage (e.g., missing per-process CPU or net rates on Darwin) or minor FFI bugs. **0:** Stubbed/absent or shells out to `top`/`ps`/`vm_stat`. |

### Category 3: Terminal Lifecycle, Interaction & Robustness — 14 points
| Item | Band | Criteria |
| :--- | ---: | :--- |
| Required controls & discoverability | 0–3 | `q`/`Esc` quit, `?` help overlay, `Space` pause/resume, `p` process diagnostics, and `Tab` focus/view cycle all work smoothly and are discoverable from the UI. |
| Terminal restoration (`q`, `Ctrl-C`, `SIGTERM`) | 0–5 | Full restoration (raw mode off, cursor visible, alternate screen exited, SGR reset) verified in PTY across normal quit (2), `Ctrl-C` / `SIGINT` (1.5), and `SIGTERM` (1.5). **Hard cap:** If normal quit or `Ctrl-C` hangs or leaves raw mode / hidden cursor / alt-screen corrupted, **Category 3 as a whole is capped at 4/14**. |
| Redirected `stdout` / non-TTY handling | 0–2 | **2:** Detects non-TTY cleanly and exits with an actionable error message (or documented non-interactive snapshot). **1:** Exits non-zero with cryptic error. **0:** Panics, hangs, or spews raw-mode escape sequences into a pipe. |
| Collector fault isolation & edge-case resilience | 0–4 | Verified via source inspection + tests: single failed collector degrades gracefully to tasteful `N/A` without crashing (1.5); handles counter rollover/reset and interface/PID churn safely (1.5); handles zero/long `dt`, permission errors, and long/non-ASCII UTF-8 names with proper column-width clipping (1.0). |

### Category 4: Build, Toolchain & Automated Tests — 10 points
| Item | Band | Criteria |
| :--- | ---: | :--- |
| Clean scratch release build | 0–2 | **2:** Compiles cleanly from clean snapshot with zero warnings. **1:** Compiles with warnings or lockfile mismatch. **0:** Build fails. |
| Formatter & static analysis | 0–2 | **1 pt** for clean `cargo fmt --check` / `gofmt -l`; **1 pt** for warning-free `cargo clippy --all-targets` / `go vet ./...`. |
| Tests pass & reproducible config | 0–2 | **1.5 pts** all automated tests pass out of the box; **0.5 pt** pinned lockfile/toolchain reproducibility. |
| Automated test coverage & determinism | 0–4 | **4:** Substantive, deterministic tests covering all four required areas—sampling/parsing, elapsed-time rate math (including edge cases), layout across terminal sizes, and terminal-independent rendering. **2–3:** Covers 2–3 areas well or misses edge cases. **1:** Token/trivial unit tests only. **0:** No meaningful tests. |

### Category 5: Responsive Layout, Color & Fallback Modes — 8 points
| Item | Band | Criteria |
| :--- | ---: | :--- |
| `80x24` compact layout, tiny-terminal safety & live resize | 0–4 | **1.5 pts** deliberately simplified `80x24` layout (not clipped `120x36`); **1.5 pts** survives `40x10` and `20x5` without panic or broken frame; **1.0 pt** smooth live `SIGWINCH` resize adaptation. |
| 24-bit color detection, monochrome & ASCII fallback | 0–4 | **1.5 pts** detects truecolor and degrades cleanly to 256/16-color without breaking layout; **1.25 pts** working monochrome/reduced-color mode; **1.25 pts** working ASCII-safe fallback with preserved LCARS composition. *(−0.5 if fallback is unreachable without recompiling or undocumented).* |

### Category 6: Performance & Render Efficiency — 6 points
| Item | Band | Criteria |
| :--- | ---: | :--- |
| Interactive startup latency | 0–1.5 | **1.5:** `< 50 ms` (well inside `< 250 ms` target). **1.0:** `50–250 ms`. **0:** `> 250 ms`. |
| Idle CPU & bounded memory | 0–2.0 | **2.0:** `< 1.0%` idle CPU and modest, bounded RSS (`< 15 MiB`) at default `500 ms–2 s` refresh. **1.0:** `1.0–3.5%` CPU or elevated RSS. **0:** `> 3.5%` idle CPU or busy-wait loop. |
| Cell-diff rendering & terminal write volume | 0–1.5 | **1.5:** True cell/span diffing with low steady-state bytes/sec and **no** full-screen clear (`\x1b[2J`) on refresh. **0.5:** Diffing claimed/partial but rewrites whole rows/frames unnecessarily. **0:** Full-screen clear/repaint every tick. |
| Simple measurement method included | 0–1.0 | **1.0:** Includes working script, benchmark flag, or exact copy-pasteable commands to measure startup, CPU, binary size, and memory. |

### Category 7: Documentation & Deliverables — 4 points
| Item | Band | Criteria |
| :--- | ---: | :--- |
| Concise README completeness | 0–2.0 | Clearly covers language decision, architecture, controls, macOS & Linux implementation details, reproducible release builds, third-party libraries, and known limitations. |
| Required deliverables & Captain's Log | 0–2.0 | **1.0 pt** Captain's Log Markdown file (or dedicated doc) summarizing what was built and verified; **0.5 pt** `120x36` primary & secondary captures delivered; **0.5 pt** exact build/test/lint/perf commands and results recorded. |

---

### Honesty Deductions (`0` to `−10` points)

`prompt-v3.md` states: *"Build and test the project before concluding. Report failures honestly. Do not substitute pseudocode, TODOs, placeholders, or omitted sections for implementation."*

Apply these as deductions (capped at `−10` total) so an untrue claim cannot be offset by points elsewhere. **Every deduction must quote the exact claim from the submission's documentation and cite the contradicting evidence.**

| Finding | Deduction |
| :--- | ---: |
| Claims a build, test count, `fmt`, or `clippy`/`vet` check passed when clean-room execution contradicts it | −3 to −4 |
| Implies native macOS execution was run and verified when it was only cross-checked or written blind | −3 |
| Claims cell-diff / flicker-free partial updates while actually clearing or repainting the full screen each tick | −2 |
| States performance numbers as measured when fabricated or contradicted by orders of magnitude on comparable hardware | −2 each (max −4) |
| Presents a narrow microbenchmark (e.g., buffer-only render time) as whole-program startup or frame performance without disclosure | −1 |
| Ships a TODO, stub, or omitted requirement while claiming full completion, or omits known limitations | −1 to −2 |

