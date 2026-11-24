# 09 app-panels

## Changes made
- `crates/app/src/spend.rs`: `spend_report` summed the per-task buckets by hand (a fold that rebuilt a record per call and added seven fields). It now maps records to their task group and reuses `summarize_costs(..).by_task`. Same totals, about 12 lines fewer, no second copy of the bucket arithmetic.
- `crates/app/src/library_panel.rs`: `home()` was `pub` but only used in this file; now private.

## Proposed, not made
- `evals_view.rs:54` reads `HOME` with `unwrap_or_default()` while `library_panel::home()` falls back to `"/"`, and `workspace.rs:3048` (layer 07) has `dirs_home()`. One shared home helper in layer 07 would remove three copies; the fallbacks differ ("" vs "/"), so unifying changes behaviour when HOME is unset.
- The `InputEvent::Change` subscription that clears the error (model_dialog, key_dialog, new_skill_dialog) repeats; a small helper in `ui.rs` (layer 07) would fold it. Only 3 copies of 5 lines, so not worth it yet.
- `spend_view::offset()` could use `Utc.fix()` instead of the `east_opt(0).expect(..)` fallback. Trivial, skipped.
- Known blocking work on the UI thread (History store writes, Skill file reads) is already listed in AGENTS.md; not touched.

## Verification
- Clippy errors that remain are in other layers' files (`ui.rs`, `editor_pane.rs`); none in layer 09 files.

## Ponytail close
Skipped: a line-by-line read of library_panel, evals_view, history_view and preferences_view (about 4000 lines); I scanned them for unused pub items (none) and duplicate function names (none), not for every local simplification. Risk: low; the spend change is covered by the existing `groups_tasks_by_their_instruction_and_sorts_by_cost` test.
