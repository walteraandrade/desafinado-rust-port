# 06 runtime

## Changes made
- `crates/runtime/src/openrouter_ai.rs`: `comment` and `target` carried two copies of the same record-and-retry loop. Both now go through one `retrying` helper. Behaviour is the same: one record per attempt, retry while the error is retryable and `attempt <= DEFAULT_MAX_RETRIES`.
- `crates/runtime/src/ai_backend.rs`: `now_iso()` is now defined once (`pub(crate)`). `openrouter_ai.rs` and the two `FakeAi` spots that inlined the same chrono call use it.
- `crates/runtime/src/ai_backend.rs`: fixed a test assertion that the current clippy rejects (`assert!(x.is_empty())` became `assert_eq!`).

## Proposed, not made
- `Services::load` reads `model.txt` the same way `model_choice` does. It could share one helper. Skipped: `load` passes the text untrimmed to `resolve_model`, which uses `js_trim`, while `model_choice` uses `str::trim`. A merge could change behaviour for odd whitespace such as a BOM.
- `Services.model` is a pub `Mutex<String>` next to the `model()` accessor. Making it private needs a check of crates/app (layer 07 or similar). I did not check every use.
- `FakeAi` (about 300 lines of test and dev fake) sits in `ai_backend.rs` next to the real trait. It could move to its own file (`fake_ai.rs`) in this crate. Skipped as churn without a behaviour gain.
- `DESAFINADO_MODEL` is read in two places (`load` and `default_model`). `load` could call `default_model`. Skipped for the same reason as the first item.

## Ponytail closing
Skipped: merging the model-file reads, moving `FakeAi`, any pub item removal (`crates/app` uses `flush_records`, `unrecorded_calls`, `model_choice`, `use_default_model`, `default_model` and `backend`). I did not run `ai-check`; it compiles only. Risk: the `retrying` closure borrows `client` and `body`. It is covered by the compiler, but the retry path itself has no test, because it needs a live client.
