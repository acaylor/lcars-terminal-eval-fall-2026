# Cross-scorecard calibration (blind)

Done by the orchestrator **after** all nine isolated graders had locked `S01.md`–`S09.md` and **before** `private/submission-map.tsv` or the human visual scores were opened. Its only purpose was to apply the rubric consistently when nine independent graders had each interpreted the same bands. Every adjustment is recorded as an addendum inside the affected scorecard. The graders' original text is left as written.

## Item matrix as locked by the graders

| Item (max) | S01 | S02 | S03 | S04 | S05 | S06 | S07 | S08 | S09 |
| :-- | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| Modules (4) | 4 | 4 | 4 | 4 | 3.5 | 3.5 | 3.5 | 3.5 | 3.5 |
| OS interface (3) | 3 | 3 | 3 | 2.5 | 2.5 | 2.5 | 3 | 3 | 3 |
| unsafe/FFI/panic (3) | 2.5 | 3 | 2.5 | 2 | 1 | 3 | 2 | 2 | 2 |
| Deps (2) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| Scope (2) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| Primary metrics (5) | 5 | 5 | 5 | 5 | 4 | 4.5 | 5 | 4.5 | 5 |
| Diagnostic metrics (4) | 4 | 4 | 4 | 2.5 | 3.5 | 2.5 | 4 | 2.5 | 4 |
| macOS collector (5) | 3.5 | 5 | 3.5 | **3.0→2.5** | 1 | **4.5→3.5** | 3.5 | 3.5 | **2.5→3.0** |
| Controls (3) | 3 | 3 | 3 | 3 | 2.5 | 3 | 3 | 3 | 3 |
| Restoration (5) | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 |
| Non-TTY (2) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| Fault/edge (4) | 4 | 4 | 4 | 4 | 2.5 | 3.25 | 3.5 | 3.5 | 3 |
| Build (2) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 1.5 |
| fmt + lint (2) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 0.5 |
| Tests pass + repro (2) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| Test coverage (4) | 4 | 4 | 3.5 | 3.5 | 3 | 3 | 3.5 | 3.5 | 3.5 |
| Layout/resize (4) | 3.5 | 4 | 4 | 4 | 3 | 3.75 | 4 | 4 | 4 |
| Color/mono/ASCII (4) | 3.5 | 4 | 4 | 4 | 3 | 3.5 | 4 | 3.5 | 4 |
| Startup (1.5) | 1.5 | 1.5 | 1.5 | 1.5 | 1.5 | 1.0 | 1.5 | 1.5 | 1.5 |
| Idle CPU/RSS (2) | 2 | 2 | 2 | 2 | 1 | 2 | 2 | 2 | 2 |
| Cell diff (1.5) | 1.5 | 1.5 | 1.5 | 0.5 | 1.5 | 1.5 | 1.5 | 1.5 | 1.5 |
| Measurement (1) | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| README (2) | 2 | 2 | 2 | 2 | 1.5 | 1.75 | 2 | 2 | 2 |
| Deliverables (2) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |

## Adjustments

| ID | Change | Reason |
| :-- | :-- | :-- |
| S04 | Total 64.0 → 63.5 | Arithmetic: the subtotals summed to 63.5 |
| S07 | Total 66.5 → 66.0 | Arithmetic: the subtotals summed to 66.0 |
| S04 | macOS 3.0 → 2.5 | Enumeration bug **plus** unscaled Mach-tick CPU (every top-5 row 0%). This is strictly worse than S09, which has the enumeration bug only |
| S06 | macOS 4.5 → 3.5 | One required metric (memory available) materially wrong. Verified: 19.8G "free" vs ≈10.2 GiB from vm_stat, and used + free exceeds total. Brought to the 3.5 level given to S01/S03/S07/S08 for a single materially wrong macOS metric |
| S09 | macOS 2.5 → 3.0 | One bug (PID buffer) affecting count and top-5 scores 3.0. Deprecation warnings are already penalised under Build and Lint. The 32-bit `if_data` counters were not penalised for S03 |

### The macOS collector ladder after calibration
5.0 complete and verified natively (S02). 3.5 one materially wrong metric: S01, S03, S07 (Mach ticks treated as ns, so process CPU% ~40× low), S06 (memory available), S08 (disk 256× from `f_bsize`). 3.0 process enumeration truncated (S09). 2.5 enumeration truncated plus CPU% unscaled (S04). 1.0 most metrics N/A or wrong from bad offsets and sysctl names (S05).

## Reviewed and left unchanged
- **Honesty: S01 −0.5 vs S09 −1.0 (both startup microbenchmarks).** S01's README discloses at :248-249 that the figure "excludes `exec` and dynamic-loader time", which contradicts its own headline "after `exec`". S09 never discloses. The partial disclosure justifies the smaller deduction.
- **No 16-colour tier:** S01 and S05 were both docked 0.5 for TERM=xterm still receiving 256-colour SGR. The two were treated alike.
- **S02 at 70/70:** I re-read it for misses using the deductions applied to other snapshots (16-colour, CJK clipping, thread counting, macOS units, diffing). Its probe data and the macOS native run back every item. The remaining nits (invalid-UTF-8 names skipped on Linux; TX counts physical NICs only, which is documented) do not reach a 0.5 band.
- **Thread-counting process lists** (S06, S08) were both scored 2.5/4 on diagnostics, so they are consistent.

## Final calibrated engineering totals

| ID | Engineering /70 | Honesty |
| :-- | --: | --: |
| S01 | 67.0 | −0.5 |
| S02 | 70.0 | 0 |
| S03 | 67.5 | 0 |
| S04 | 63.0 | 0 |
| S05 | 55.0 | −1.0 |
| S06 | 62.25 | 0 |
| S07 | 66.0 | 0 |
| S08 | 63.5 | 0 |
| S09 | 63.0 | −1.0 |
