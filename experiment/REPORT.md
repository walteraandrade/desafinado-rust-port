# Layered ponytail review: report

Split: [LAYERS.md](LAYERS.md). Per-layer reports: [layers/](layers/).

## Layer split

| # | Layer | Owns | Tracked files | Net diff |
|---|---|---|---|---|
| 01 | core | `crates/core/**` | 27 | +8 −42 (3 files) |
| 02 | ai | `crates/ai/**` | 61 | +0 −3045 (32 files, 25 deleted) |
| 03 | features-skills | `crates/features/src/skills/**` | 22 | +3 −123 (2) |
| 04 | features-domain | rest of `crates/features/**` | 61 | +3 −119 (3) |
| 05 | platform | `crates/platform/**` | 51 | +7 −149 (3, 1 deleted) |
| 06 | runtime | `crates/runtime/**` | 6 | +60 −76 (2) |
| 07 | app-shell | app `main`, `workspace`, `ui`, `theme`, startup files, assets, tests, `Cargo.toml`, `build.rs` | 22 | main.rs only |
| 08 | app-editor | app editor views (10 files) | 10 | `editor_pane.rs`, `comment_review.rs` |
| 09 | app-panels | app panels and dialogs (11 files) | 11 | `spend.rs`, `library_panel.rs` |
| 10 | vendor-gpui-base | `vendor/gpui-base/**` | 230 | none |
| 11 | tooling-docs | root config, CI, scripts, docs, packaging, spike | 117 | +1 −1 (`docs/ui-rubric.md`) |

All 618 tracked files map to exactly one layer (checked with a script over `git ls-files`). App layers 07–09 together: +48 −75 in 5 files.

## Per layer

**01 core.** Changed: deleted `clear_suggestions` (no caller) and its tests; put `accept_suggestion`, `discard_suggestion`, `in_flight_suggestion_ids` under `#[cfg(test)]` (only tests call them); removed unused `serde_json` and `thiserror` from `crates/core/Cargo.toml`. Proposed: narrow core-only pub items.

**02 ai.** Changed: deleted 24 unreferenced modules (12 in `editor/`, 12 in `ui/`) plus `dev_log.rs`, about 3000 lines; removed unused `CallResult`, `DECISION_PROVIDER`, `MIN_RETRY_MS` and motion constants in `ui/list_motion.rs`. Proposed: narrow file-local pub items; make `regex` a workspace dependency.

**03 features-skills.** Changed: removed `is_same_report` and `announce_problems` from `frontmatter_panel.rs` and their tests. Proposed: audit the two big test files for redundant tests.

**04 features-domain.** Changed: removed the unused History subscription machinery (`subscribe`, `unsubscribe`, `SubscriptionId`, `notify`) in `history/hub.rs` and `history/session.rs`; removed `six_digit_hex` in `theme/color.rs`. Proposed: platform reuses its `thin`, `word_delta`, `types`; narrow ~20 pub items; check whether `store_contract` is test-only.

**05 platform.** Changed: deleted `history/thin.rs`; `history/db.rs` now uses `desafinado_features::history::thin`. Proposed, not made: unify `word_delta` (different diff algorithm, values stored in SQLite) and `types` (stricter enum could reject old rows) only after an equivalence or migration test.

**06 runtime.** Changed: `comment` and `target` share one `retrying` helper instead of two copies of the record-and-retry loop; `now_iso()` defined once; one test assert rewritten for the new clippy lint. Proposed: share the `model.txt` read (skipped: `js_trim` vs `str::trim` differ), make `Services.model` private, move `FakeAi` to its own file.

**07 app-shell.** Changed: `main.rs` folds the startup repeated in `main` and `show_start_error` into `run_app(setup, build)`, same call order. Proposed: split `workspace.rs` and `ui.rs`; move `futures` and `chrono` to workspace dependencies.

**08 app-editor.** Changed: dropped the unused `window` parameter (and two `let _ = window;`) from `ask_with` and `step_change`, callers updated in `editor_pane.rs` and `comment_review.rs`; one test assert rewritten for the new lint. Proposed: a focus-sync helper in `frontmatter_view.rs` (judged not worth a trait).

**09 app-panels.** Changed: `spend_report` reuses `summarize_costs(..).by_task` instead of a hand-written fold; `library_panel::home()` made private. Proposed: one HOME helper in layer 07 (fallbacks differ: `""` vs `"/"`), an error-clearing subscription helper in `ui.rs`, `Utc.fix()` in `spend_view`.

**10 vendor-gpui-base.** No change. The patch matches `PATCH.md` and has no dead items.

**11 tooling-docs.** Changed: `docs/ui-rubric.md` "Seven" → "Eight" (the sentence lists eight CI-checked criteria). Proposed: about 90 unreferenced PNGs in `docs/screenshots/` (9.4 MB); unreferenced `docs/benchmarks/review-mode-decision.md`.

## Conflicts and resolutions

1. **`clippy::assert_is_empty` (9 layers).** The installed toolchain is Rust 1.100; CI pins 1.97.0. Clippy 1.100 adds `assert_is_empty`, which fails 67 `assert!(x.is_empty())` test sites in 5 crates, all present before this work. Layers 06 and 08 rewrote their own sites to `assert_eq!`; layers 01–05, 07 proposed a workspace allow, `#[expect]`, or a toolchain pin. Resolved by the lead in root `Cargo.toml` (layer 11's file): `assert_is_empty = "allow"` with a reason comment, the house pattern for workspace lint choices. Cost: clippy 1.97 (CI) prints a non-fatal `unknown lint: clippy::assert_is_empty` warning per crate; `cargo +1.97.0 clippy --workspace --all-targets -- -D warnings` still finishes. The 06 and 08 rewrites stay (harmless).
2. **`Cargo.lock` edited outside its layer.** Layer 01 removed two dependencies; cargo rewrote `Cargo.lock` (layer 11). Accepted: it is a generated side effect, two lines, and the only consistent state.
3. **History duplicates, 04 vs 05.** Layer 04 proposed platform reuse all three of `thin`, `word_delta`, `types`; layer 05 reused `thin` only. Resolved in favor of 05: I compared both `thin` versions (same buckets, same "edited and not saved" filter, same `newest` floor at 0) and the `CauseKind` mapping in `db.rs`; `word_delta` and `types` stay duplicated until a test proves equal output on stored data.
4. **HOME helper, 09 → 07.** Layer 09 proposed a shared helper in 07's `workspace.rs`; 07 did not know. Not made: the three fallbacks differ, so unifying changes behavior when `HOME` is unset.
5. **Deleted ai modules vs app copies.** Layer 02 deleted `ai::ui::menu_nav` and `ai::editor::minimal_change`; the app has its own live copies (`crates/app/src/menu_nav.rs`, `document.rs`). The full workspace build confirms nothing referenced the deleted ones.
6. **`regex`, 02 vs 11.** Not a contradiction: 02 proposes moving `regex = "1"` (in `ai` and `features`) into workspace dependencies; 11 found no unused workspace dependency. Not made (no behavior gain).
7. **Transient breakage during the parallel run.** Layers 03, 07, 09 saw errors in other layers' files mid-edit (an unused import in `history/hub.rs`, `ui.rs`, `editor_pane.rs`). All were gone at integration.

## Did each subagent follow its instructions?

Checked from each transcript and `git status`:

| Layer | Read the skill | Stayed in its files | Wrote its report | Notes |
|---|---|---|---|---|
| 01 | yes | yes, except the cargo-generated `Cargo.lock` lines | yes | |
| 02 | yes | yes | yes | read `ai-check.rs` with `cat`; never ran it |
| 03 | yes | yes | yes | |
| 04 | yes | yes | yes | ran read-only `git stash list` |
| 05 | yes | yes | yes | |
| 06 | yes | yes | yes | built `ai-check`, never ran it |
| 07 | yes | yes | yes | ran read-only `git stash list` |
| 08 | yes | yes | yes | |
| 09 | yes | yes | yes | |
| 10 | yes | yes (no edits) | yes | |
| 11 | yes | yes | yes | |

No subagent ran `cargo fmt --all`, the app, `smoke.sh`, or a git write command.

## Final checks (repo root, Rust 1.100 stable)

- `cargo fmt --all`: changed nothing; `cargo fmt --all -- --check` clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (after resolution 1). Also clean with `cargo +1.97.0` (CI's toolchain), with the warning noted above.
- `cargo test --workspace`: all pass. 1895 tests: core 377, ai 291, features 816, platform 310 (1 ignored, as before), runtime 15, app 78 + 8 (`ui_rules`), 0 failed.
- Not run: the app, `scripts/smoke.sh`, `ai-check`, the `deny` job, the `gpui-base` test job (vendor untouched).

## Net lines

`git diff --stat` before the commit (tracked files, excluding `experiment/`): **53 files changed, 132 insertions(+), 3632 deletions(-)**. Net −3500 lines, ~3000 of them the ai modules.

## Risks

- Dead-code detection was by name grep, plus the compiler on the full workspace. Code reachable only by a path outside this repo is gone (it stays in git history).
- The 01 `#[cfg(test)]` functions are tested but not shipped; deleting them with their tests would be the fuller ponytail move.
- The `retrying` path in runtime has no test of its own (needs a live client); the change is a mechanical extraction.
- Window startup after the `run_app` refactor was not run here; CI's smoke job covers it.
