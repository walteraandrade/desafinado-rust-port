# Model baselines

Numbers from real `ai-check` runs, kept so a new model can be measured against the one in use. Add a dated section per run; do not edit old ones.

To compare a candidate: change the model constant (`DECISION_MODEL` in `crates/ai/src/review_mode.rs` for the review mode), run `cargo run -p desafinado-runtime --bin ai-check`, and add its numbers below. Compare the choice, the probability, the cost per call and the client latency on the same Focus texts.

## Review mode decision

The decision model reads the Focus of a Review and answers `comment` or `revise`.

### 2026-11-22: `typesafe/jev-1.14` (commit `ca2cb65`)

Served by `typesafe/jev-1.14-20261014` · TypeSafe, through the OpenRouter Decisions API.

| Focus | Expected | Answer | Probability | Tokens in / out | Cost (US$) | Client latency (ms) |
| --- | --- | --- | --- | --- | --- | --- |
| what do you think of the tone? | Comment | Comment | 100% | 416 / 32 | 0.000017 | 444 |
| fix the spelling | Revise | Revise | 100% | 411 / 33 | 0.000017 | 400 |

2 of 2 correct. Two Focus texts are a smoke test, not an accuracy measure: a candidate that matches these still needs harder cases (a Focus that mixes both, such as "is the tone right? fix it if not"). The Decisions API returns no generation id, so only the client latency is known.

## Chat model, same run

For scale: the other calls in the same run, on `openai/gpt-6.1-terra` (served by `openai/gpt-6.1-terra-20261026` · OpenAI).

| Task | Tokens in / out | Cost (US$) | Client latency (ms) | Result |
| --- | --- | --- | --- | --- |
| Suggestion, Shorter on p1 | 515 / 82 | 0.000925 | 2444 | target ok |
| Comment review | 1033 / 356 | 0.003070 | 6101 | 1 general note, 6 notes, 1 dropped |
| Review pass | 975 / 318 | 0.002565 | 4603 | 5 edits |

Total for the run: 5 calls, US$0.006595. Each Jev decision costs about 1/180 of the Comment review it routes.
