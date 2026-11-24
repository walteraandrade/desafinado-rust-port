# 10 vendor-gpui-base

## Changes made
None. The patch is already minimal: `inlays.rs` has only the types and `shift_offset`, all used by the patched code and by `crates/app/src/editor_pane.rs` (`Inlay`, `BlockZone`). No dead items found among the patched parts. Diff vs pristine 0.9.1 `src` is about 812 changed lines, all described in PATCH.md; no other file differs besides PATCH.md.

## Proposed, not made
None.

## Closing
- Checked: `cargo fmt --check` clean, `cargo clippy --offline --features test-support --lib` no warnings, in vendor/gpui-base. Did not run the test suite (no code changed).
- Did not review every patched hunk line by line; any edit risks making a gpui-kit bump harder, so I left the code alone.
