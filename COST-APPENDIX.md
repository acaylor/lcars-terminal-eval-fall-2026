# Cost & Effort Appendix (non-scoring)

> **This appendix does not affect any score.** It collects the token, duration, harness and cost data that each run left in its original `cost.txt`, for the nine evaluated models only. Three runs (`gemini-4`, `gpt-6-astra`, `opus-5.5`) shipped no `cost.txt`. Their cost fields are left blank rather than guessed.

## Recorded data (verbatim from `cost.txt`)

| Model | Harness | Reported cost | Tokens / context | Reported duration | Notes |
| :-- | :-- | --: | :-- | :-- | :-- |
| DeepSeek V4.1 Flash 1017 | OpenCode 1.19.4 (OpenRouter, "high") | **$1.78** | 436,512 context tokens (42% used) | n/r | |
| Claude Fable 5.1 | Claude Code v2.1.310 (medium effort) | **$14.89** | Fable: 1.6k input, 162.0k output, 8.9M cache read, 227.3k cache write; Haiku helper $0.0001 | 31m 54s API / 39m 21s wall | 97% of input tokens served from prompt cache |
| Gemini 4 Argon | n/r | n/r | n/r | n/r | no `cost.txt` |
| GPT-6 Astra | n/r | n/r | n/r | n/r | no `cost.txt` |
| GPT-6.1 Sol | n/r | n/r | n/r | "Worked for 36m 4s" | only a duration line was recorded |
| Grok 4.7 | n/r | **$5.53** | 204,023 context tokens (41% used) | n/r | |
| Kimi K3.1 Pro | OpenCode 1.19.4 (OpenRouter, "high") | **$1.55** | 106,059 context tokens (10% used) | 12m 52s | |
| Muse Wonder | OpenCode 1.19.4 (OpenRouter, "high") | **$5.09** | 109,129 context tokens (10% used) | n/r | |
| Claude Opus 5.5 | n/r | n/r | n/r | n/r | no `cost.txt` |

n/r = not recorded.

## Unofficial session-span estimate (file mtimes)

Measured as the time from when `prompt-v3.md` was placed in the directory to the newest file in the submission, excluding `target/`, `.git/` and `cost.txt`. This is an upper bound and a rough one: it includes any idle time and any post-run copying. Where a recorded duration exists, the two agree within about 3 minutes in two of three cases (Fable 38 vs 39 min; GPT-6.1 Sol 35 vs 36 min) but not for Kimi (39 vs 13 min).

| Model | Prompt placed | Span to last file |
| :-- | :-- | --: |
| DeepSeek V4.1 Flash 1017 | 2026-10-27 21:06 | ~46 min |
| Claude Fable 5.1 | 2026-10-27 22:26 | ~38 min |
| Gemini 4 Argon | 2026-10-20 07:24 | ~96 min |
| GPT-6 Astra | 2026-09-06 22:33 | ~28 min |
| GPT-6.1 Sol | 2026-09-30 07:44 | ~35 min |
| Grok 4.7 | 2026-09-22 21:28 | ~59 min |
| Kimi K3.1 Pro | 2026-10-27 16:25 | ~39 min |
| Muse Wonder | 2026-10-27 19:57 | ~14 min |
| Claude Opus 5.5 | 2026-09-22 17:39 | ~42 min |

## Caveats
- Harnesses report "cost" differently. OpenCode shows OpenRouter spend; Claude Code shows list-price API cost including cache reads and writes. The figures are not normalized.
- The context-window percentages are harness-specific and are not total tokens consumed.
- The runs span September–October 2026 with different harness versions.
