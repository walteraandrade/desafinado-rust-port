# 03 features-skills

## Changes made
- `crates/features/src/skills/frontmatter_panel.rs`: removed `is_same_report` (a wrapper over `==`) and `announce_problems`. Neither had a caller outside tests; the whole repo was grepped. `announce_rule` stays (used by `new_skill.rs`).
- `crates/features/src/skills/frontmatter_panel_tests.rs`: removed the tests and the `skill` helper that only served those two functions. The `announce_rule` phrase test stays.

## Proposed, not made
- Layer 04 / toolchain: `cargo clippy -p desafinado-features --all-targets -- -D warnings` fails on `assert_is_empty` (newer clippy) in `history/*`, `evals/settings.rs` and an unused import in `history/hub.rs`, plus 7 spots in this layer (`library_tree.rs:376,492,494`, `skill_template.rs:106`, `frontmatter_panel_tests.rs`). They predate this change. I left them: the fix is lint-version dependent (pin the toolchain or allow-list in the workspace `Cargo.toml`, owned by layer 04).
- The big files (`frontmatter_fields_tests.rs` 1.4k lines, `scan_skills_tests.rs` ~1k) were not audited for redundant tests.

## Closing
Skipped: line-by-line review of the ~9k remaining lines; only pub items with zero references were hunted. Risk: low, the removed items had no production callers. Not verified: a full-workspace build, since only the crate was checked.
