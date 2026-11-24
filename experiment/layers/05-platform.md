# 05 platform

## Changes made
- `crates/platform/src/history/thin.rs` deleted; `db.rs` now uses `desafinado_features::history::thin::{ThinCandidate, ids_to_thin}`. The algorithm is identical (only a fold and an enum `kind` differ). `db.rs` maps the stored cause to `CauseKind::Edited` or `External`; `thin` only checks `Edited`. About 145 lines removed, with their duplicated tests (the features copy keeps them). `history/mod.rs` no longer exports `ThinCandidate` or `ids_to_thin` (grep: no other user).

## Proposed, not made
- `history/word_delta.rs` vs `features/history/word_delta.rs`: NOT equivalent. Platform diffs lines with `similar` (Myers, 250 ms deadline); features uses `line_runs` + `crate::diff::diff_tokens`. Word counts could differ on edge cases, and the platform values are stored in SQLite. Proposal (layer 04 + 05): make features' `word_delta` the single implementation after confirming equal outputs on a corpus, then delete the platform copy and the `similar` use here.
- `history/types.rs`: not forced. Platform `AcceptedChange.origin` is a `String` and `VersionSummary` has `i64` words with `cause` and `saved` fields; features has a `SuggestionOrigin` enum and `usize`. Reading existing DB rows into the stricter enum could fail on old data (a data-loss-adjacent risk). Proposal: layer 04 adds a lenient deserialize or a migration test, then platform reuses `VersionCause` and `CauseKind`.
- Pre-existing clippy failure in test targets (not caused by this change): 13 `assert!(x.is_empty())` sites flagged by the current toolchain, in evals/stage.rs:221, files/save.rs:225, history/db.rs:808 and :836, library/create_skill.rs:150, library/handlers.rs:210, library/scan.rs:185, :196 and :325, records.rs:118, watch/controller.rs:564, :618 and :661. Left alone because they are mechanical and noisy; `cargo clippy -p desafinado-platform -- -D warnings` (lib only) is green.

## Closing
Skipped: a full read of the evals, library and key modules (about 6k lines); no atomic-save, keyring or error-handling code was touched. Risk: low, since only the `thin` call site changed; the platform tests (thin via `HistoryDb::thin`) pass.
