# 04 features-domain

## Changes made
- `crates/features/src/history/hub.rs`, `history/session.rs`: removed the subscription machinery (`subscribe`, `unsubscribe`, `SubscriptionId`, `listeners`, `notify`). No caller in the repo outside these two files' own tests; `settled()` already returns `bool`, which is the signal callers use. Removed the two subscriber-only tests and trimmed two others to assert on `settled()`/store calls instead.
- `crates/features/src/theme/color.rs`: removed `six_digit_hex`, used nowhere in the repo.

## Proposed, not made
- Layer 05 `crates/platform/src/history/{thin,word_delta,types}.rs`: my `history/{thin,word_delta,types}.rs` are fit to be the single copy. They are typed (`CauseKind`, `usize` counts), reuse `crate::diff` and `line_runs`, and carry the same tests plus a time-budget test. Differences layer 05 must absorb: `ThinCandidate.kind` is `CauseKind` (platform: `String`), `WordDelta` fields are `usize` (platform: `i64`), and platform's `count_words` wrapper and its test would go. I kept the public API unchanged.
- Narrow to private/`pub(crate)` (no use outside the crate, only internal): `diff_tokens`, `significant_tokens`, `DiffPart`, `matches_skill_name`, `is_valid_model`, `tally_of`, `is_cause`, `AI_SHARE`, `apply_write`, `roles_as_colors`, `roles_of_seed`, `*_CONTRAST` consts, `MIN_RUNS`/`MAX_RUNS`/`MAX_COST_LIMIT_USD`, `Rgb`. Skipped: churn with no behaviour gain, and some are used by sibling modules/tests.
- `run_store_contract`/`StoreContractOptions` are only used outside the crate? Not verified; check whether `history/store_contract.rs` should be `#[cfg(test)]` or feature-gated.

## Verification
- `cargo clippy -p desafinado-features --all-targets -- -D warnings` fails only on `clippy::assert_is_empty` in tests (rustc 1.100), present before my change: my files `evals/settings.rs` (3), `history/{events,hub,session,store_contract,timeline}.rs`, plus layer 03's `skills/*`. I did not touch them; fixing means `assert_eq!(x, Vec::<_>::new())`-style churn, or a toolchain pin decision.
- rustfmt check clean on edited files. Tests: see summary.

## Skipped / risks
- Did not read every file line by line; the dead-code pass was by grep of pub items across crates. Names matched by word only, so a same-named item elsewhere could hide dead code.
- Removing subscriptions is irreversible for any out-of-tree caller; none in this repo.
