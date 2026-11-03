# SXX — Engineering Scorecard (anonymous, LOCKED)

- **Language / toolchain:** …
- **Build gate:** PASS / FAIL
- **Engineering total:** **NN.N / 70**
- **Honesty deductions:** **−N.N**
- **Engineering net (70 − deductions applied later to the 100-pt total):** NN.N

## Score table

| Category | Item | Max | Score | Evidence (short) |
| :-- | :-- | --: | --: | :-- |
| 1 Architecture & Scope | Module structure & separation | 4 | | |
| | OS collector interface isolation | 3 | | |
| | unsafe/FFI hygiene, panic safety, completeness | 3 | | |
| | Dependency minimalism & justification | 2 | | |
| | Scope discipline | 2 | | |
| **1 subtotal** | | **14** | | |
| 2 Metrics & Collectors | Primary metrics & elapsed-time rate math (Linux) | 5 | | |
| | Secondary diagnostic metrics (Linux) | 4 | | |
| | macOS collector implementation | 5 | | |
| **2 subtotal** | | **14** | | |
| 3 Lifecycle & Robustness | Controls & discoverability | 3 | | |
| | Terminal restoration (q 2 / Ctrl-C 1.5 / SIGTERM 1.5) | 5 | | |
| | Non-TTY handling | 2 | | |
| | Collector fault isolation & edge cases (1.5/1.5/1.0) | 4 | | |
| **3 subtotal** (cap 4 if hard-cap triggered) | | **14** | | |
| 4 Build & Tests | Clean scratch release build | 2 | | |
| | Formatter (1) & static analysis (1) | 2 | | |
| | Tests pass (1.5) & reproducible config (0.5) | 2 | | |
| | Test coverage & determinism | 4 | | |
| **4 subtotal** | | **10** | | |
| 5 Layout & Fallbacks | 80x24 (1.5) / tiny (1.5) / live resize (1.0) | 4 | | |
| | Truecolor+degrade (1.5) / mono (1.25) / ASCII (1.25) | 4 | | |
| **5 subtotal** | | **8** | | |
| 6 Performance | Startup latency | 1.5 | | |
| | Idle CPU & bounded memory | 2 | | |
| | Cell-diff rendering & write volume | 1.5 | | |
| | Measurement method included | 1 | | |
| **6 subtotal** | | **6** | | |
| 7 Docs & Deliverables | README completeness | 2 | | |
| | Captain's log (1) / captures (0.5) / commands+results (0.5) | 2 | | |
| **7 subtotal** | | **4** | | |
| **TOTAL** | | **70** | | |

## Honesty deductions

| Quoted claim (file:line) | Contradicting evidence | Deduction |
| :-- | :-- | --: |

## Source inspection notes (file:line)

### Architecture & main loop
### OS collector interface & macOS backend
### Forbidden-subprocess check
### Rate math & robustness edge cases
### unsafe / FFI / panic audit & bounded memory
### Test quality & determinism
### Scope discipline

## PTY runtime, metric accuracy & lifecycle results (Linux)

## macOS cross-check results (native run on macOS host)

## Linux performance measurements

## Documentation & deliverables
