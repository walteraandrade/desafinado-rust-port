# 01 core (desafinado-core)

## Changes made
- `crates/core/src/suggestions/state.rs`, `mod.rs`: deleted `clear_suggestions` (no caller anywhere, only its 2 tests).
- Same files: `accept_suggestion`, `discard_suggestion`, `in_flight_suggestion_ids` (+ private `is_in_flight`) had no production caller in the repo, only tests; now `#[cfg(test)]` and re-exported under `#[cfg(test)]`.
- `crates/core/Cargo.toml`: removed unused `serde_json` and `thiserror` (rustc `unused_dependencies` warned; fails `-D warnings`). Side effect: 2 lines gone from `Cargo.lock`.

## Proposed, not made
- Test-only: 11 `assert!(x.is_empty())` trip the new clippy `assert_is_empty` on rustc 1.100 (adaptive_diff.rs 356/367, moments.rs 80/86/125, state.rs 985/1895/1946/2176/2184/2240). They existed before my change, so `clippy --all-targets -D warnings` fails for core. Fix is mechanical (typed empty in `assert_eq!`) or allow the lint workspace-wide in root `Cargo.toml` (owner: workspace/CI layer).
- `text_map::total_rows`, `MapBar`-style items are pub but only used inside core; left alone (cheap to narrow, but widens diffs in other layers' view of the API).
- `Cargo.lock` change should be committed with this layer.

## Skipped / risks
- Did not read every file line by line (8.4k lines); dead-code search was by symbol grep over `crates/`, so a use via macro or string would be missed.
- `cfg(test)` on those fns assumes no external crate's tests use them (grep found none).
- Verification: `cargo test -p desafinado-core` and `cargo clippy -p desafinado-core -- -D warnings` (lib) pass; `--all-targets` fails only on the pre-existing lint above.
