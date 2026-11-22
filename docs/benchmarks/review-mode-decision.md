# Review mode decision: baseline

A Review with a Focus asks a decision model whether the writer wants an editor's comments (Comment) or changes to the text (Revise). The code is `crates/ai/src/review_mode.rs`. This file keeps the numbers of the model in use, so a competing model can be compared against them later.

## Current model

| | |
|---|---|
| Model | `typesafe/jev-1.14` (served as `typesafe/jev-1.14-20261014`, provider TypeSafe) |
| Endpoint | `POST https://openrouter.ai/api/alpha/decisions` (not chat completions) |
| Listed price | US$0.042 per 1M input tokens, output free |
| Listed latency | P50 about 0.09 s |
| Request limit | 64k tokens in default mode |

## Measured runs

Taken with `ai-check` (`cargo run -p desafinado-runtime --bin ai-check`), which asks the two Focus texts below. Latency is the client's total, from the maintainer's machine through OneCLI.

| Date | Commit | Focus | Expected | Answer | Probability | Tokens in / out | Cost (US$) | Latency (ms) |
|---|---|---|---|---|---|---|---|---|
| 2026-11-22 | ca2cb65 | what do you think of the tone? | Comment | Comment | 100% | 416 / 32 | 0.000017 | 444 |
| 2026-11-22 | ca2cb65 | fix the spelling | Revise | Revise | 100% | 411 / 33 | 0.000017 | 400 |

For scale, the Comment review that follows the decision cost US$0.003070 and took 6101 ms on `openai/gpt-6.1-terra` in the same run, so the decision adds about 0.6% to the cost and 7% to the wait.

The decisions API returns no generation id, so these records carry no OpenRouter latency or generation time.

## Comparing another model

A replacement should match on accuracy first, then on cost and latency:

1. Run `ai-check` with the candidate and add its rows above, with the date and commit.
2. Two prompts only show that the wiring works. Before switching, also try harder Focus texts, such as "make it better", "check the tone and fix the typos", "is this too long?" and Portuguese requests like "enxugue o texto", and write down what each model answered.
3. Note the price per call, the latency and whether the probability is useful for the header chip.
