# LCARS terminal monitor challenge, Fall 2026: engineering evaluation

Nine coding models were each given the same brief ([`prompt-v3.md`](prompt-v3.md)): build an LCARS-styled terminal system monitor for Linux and macOS in Rust or Go. This repository holds the clean-room engineering evaluation of what they delivered.

**Read the report: https://acaylor.github.io/lcars-terminal-eval-fall-2026/**

| Rank | Model | Visual /30 | Engineering /70 | Honesty | Final |
| --: | :-- | --: | --: | --: | --: |
| 1 | Claude Opus 5.5 | 28.5 | 70.0 | 0 | **98.5** |
| 2 | Claude Fable 5.1 | 27.0 | 67.0 | −0.5 | **93.5** |
| 3= | GPT-6.1 Sol | 24.0 | 67.5 | 0 | **91.5** |
| 3= | GPT-6 Astra | 25.5 | 66.0 | 0 | **91.5** |
| 5 | Gemini 4 Argon | 28.5 | 63.0 | −1.0 | **90.5** |
| 6 | DeepSeek V4.1 Flash 1017 | 22.5 | 63.5 | 0 | **86.0** |
| 7 | Grok 4.7 | 22.5 | 63.0 | 0 | **85.5** |
| 8 | Kimi K3.1 Pro | 21.0 | 62.25 | 0 | **83.25** |
| 9 | Muse Wonder | 19.5 | 55.0 | −1.0 | **73.5** |

Visual scores are human judgements. Engineering scores were graded blind by isolated agents against anonymized snapshots (S01–S09) and locked before the model mapping or visual scores were opened.

> **Conflict of interest.** The evaluator and all nine blind graders ran on Claude Opus 5.5, which is also the winning contestant. See [`LEADERBOARD.md`](LEADERBOARD.md) for the safeguards and how to weigh them.

## What's here

| Path | Contents |
| :-- | :-- |
| [`LEADERBOARD.md`](LEADERBOARD.md) | Ranked results, category breakdown and the decisive findings per model |
| [`FINAL-REPORT.md`](FINAL-REPORT.md) | Cross-cutting synthesis: the macOS audit, performance, lifecycle, honesty deductions, patterns |
| [`METHODOLOGY.md`](METHODOLOGY.md) | Hosts, toolchains, the clean-room protocol and every probe |
| [`COST-APPENDIX.md`](COST-APPENDIX.md) | Cost and effort data reported by each run (not scored) |
| [`RUBRIC.md`](RUBRIC.md), [`SCORECARD-TEMPLATE.md`](SCORECARD-TEMPLATE.md) | What the blind graders scored against |
| [`results/`](results/) | The nine locked scorecards and the blind calibration record |
| [`scripts/`](scripts/) | Snapshot setup, build/test/lint, PTY and performance probes, and the report build |
| [`artifacts/`](artifacts/) | Raw probe output per snapshot: build and test logs, PTY probe JSON, performance JSON, OS reference snapshots |
| [`captures-fresh/`](captures-fresh/) | Screen captures from the probes as `.txt`, replayable `.ansi`, `.svg` and `.png`; `macos/` holds the native macOS run |
| [`work/`](work/) | The anonymized snapshots that were graded (source only) |
| [`private/`](private/) | The snapshot-to-model mapping, sealed during grading |
| [`docs/`](docs/) | The published HTML report |

`private/submission-map.tsv` maps the anonymous IDs: S01 Fable 5.1, S02 Opus 5.5, S03 GPT-6.1 Sol, S04 Grok 4.7, S05 Muse Wonder, S06 Kimi K3.1 Pro, S07 GPT-6 Astra, S08 DeepSeek V4.1 Flash, S09 Gemini 4.

## Changes made for publication

The evaluation ran on the author's own machines. Before publishing, identifiers for those machines and their network were replaced with consistent stand-ins in every file, including the screen captures, which were redrawn from their cell data so layouts are unchanged:

- Hostnames became `linux` and `mac-host`; home directories became `/home/user` and `/Users/user`, and the workspace path became `<workspace>`.
- Names of processes that described the host's services and tools became `svc01`, `svc02` and so on, mapped consistently across files. Common system processes (for example `systemd`, `launchd`, `kernel_task`, `python`) and the monitors themselves keep their names.
- Network interface names became generic ones such as `eth0` and `veth03`, and private IP addresses moved into the `192.0.2.0/24` documentation range.
- Compiled binaries were removed from `work/` and `artifacts/`.

The replacements also apply inside `work/`, where a few snapshots had recorded the evaluation host's name or copied its `/proc/net/dev` into test data. Nothing else in the snapshots was changed. Hardware models, OS versions, timestamps and the measured figures are as recorded.

## Rebuilding the report

```sh
python3 scripts/report/build.py
```

This regenerates `docs/index.html` from `scripts/report/template.html` and the capture SVGs.
