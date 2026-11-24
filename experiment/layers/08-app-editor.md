# 08 app-editor

## Changes made
- `editor_pane.rs`: `ask_with` and `step_change` took a `window` they never used (kept alive with `let _ = window;`). Dropped the parameter and the two `let _ =` lines, and updated the three callers (`bubble_confirm`, `press`, the step button).
- `comment_review.rs`: caller of `ask_with` no longer passes `window`.
- `editor_pane.rs` test `space_types_in_the_refine_field`: `assert!(..is_empty())` became `assert_eq!(.., Vec::<&str>::new())`, because the toolchain's `assert_is_empty` lint rejected it under `-D warnings`.

## Proposed, not made
- Layer 07, `ui.rs` tests (about lines 1780, 1873, 1880): 4 `assert!(..is_empty())` errors under `cargo clippy --all-targets -D warnings`. They block the all-targets clippy run for the whole crate.
- `frontmatter_view.rs::refresh`: the name and description branches repeat "if not focused and value differs, set_value". A small generic helper would cut about 12 lines, but the two states are different types (`InputState`, `TextareaState`), so it is not worth a trait.
- Known sync-I/O-on-UI-thread spots listed in AGENTS.md (`caret::init`, open and reload reads) belong to a move into plain crates, not a deletion.

## Closing
Skipped: a deep read of `editor_pane.rs` (3.3k lines) and of `spelling.rs`, `comment_review.rs` and `text_map_view.rs`. I found no dead items (an unused-name grep across the crate and tests turned up only test fns) and made no larger restructuring. Verified: `cargo clippy -p desafinado -- -D warnings` is clean; `cargo test -p desafinado` passes. `--all-targets` clippy still fails only in `ui.rs` (layer 07). Risk: low, since the change only removes unused parameters.
