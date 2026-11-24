# 07 app-shell

## Changes made
- `crates/app/src/main.rs`: `show_start_error` and `main` repeated the same ~15 lines (application, assets, `gpui_kit::init`, fonts, window options, open window, quit on last close, activate). Folded into one `run_app(setup, build)`. The init order is unchanged (init, fonts, setup, window). `user_data` is cloned once so both closures can own what they need.

## Proposed, not made
- `cargo clippy -p desafinado --all-targets -- -D warnings` fails on the current toolchain (clippy `assert_is_empty`, 5 hits), none from this change: `crates/app/src/ui.rs` tests at lines 1769, 1780, 1873, 1880 (mine, left as is because the lint's suggested fix is worse than the test; use `#[expect(clippy::assert_is_empty, reason = "...")]` on the test module or pin the toolchain) and `crates/app/src/editor_pane.rs:3300` (layer 08/09).
- `menu_nav::shifted` and `next_enabled` are single-use but tested and tiny; left.
- `workspace.rs` (3774 lines) and `ui.rs` (1882 lines) are the real size problem; splitting them is a structural change, not a ponytail deletion, and other layers edit around them.
- `Cargo.toml`: `futures = "0.3"` and `chrono` are pinned outside `[workspace.dependencies]`, unlike the other deps; moving them is a root `Cargo.toml` change (not my file).

## Skipped / not checked
- Did not read `workspace.rs`, `ui.rs` and `theme.rs` line by line for dead code; the compiler's `dead_code` lint passes on the binary, and other layers use their items.
- `ui_rules.rs` untouched.
- Risk: low. `run_app` keeps the order of calls. Not run (by rule), so window startup is untested.
