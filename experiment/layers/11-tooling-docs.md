# 11 tooling-docs

## Changes made
- `docs/ui-rubric.md`: "Seven are tests that run in CI" -> "Eight". The sentence lists seven `ui_rules.rs` criteria plus C5 in `theme.rs`, so eight.

## Proposed, not made
- `docs/screenshots/`: 9.4 MB, about 90 PNGs (ask-bubble-*, caret-*, comment-review-*, inline-diff-*, motion-*, place-*, spell-*, timeline-strip-*, rust-spike-* and others) are named in no file in the repo. Only `ui-rubric-*` is referenced (by `docs/ui-rubric.md`). They look like PR evidence, possibly linked from old PR bodies. Not deleted. Owner decides: delete the unreferenced ones, or keep them as PR evidence.
- `docs/benchmarks/review-mode-decision.md`: referenced nowhere. Kept; if it is a one-off record, link it from `docs/PARITY.md` or drop it.
- Root `Cargo.toml`: every `[workspace.dependencies]` entry is used by at least one member crate (grepped each). Nothing to remove. The `[profile.dev.package.*]` entries (spellbook, harper-core, fst, miniz_oxide) were left; the comment above `[profile.release]` actually describes the release profile but sits above the dev-profile block, a cosmetic misplacement I did not reorder.
- No dead config found in `clippy.toml`, `deny.toml`, `.sonarcloud.properties`, `.github/**`. CI steps all do something (the separate `cargo build -p desafinado` is needed because `cargo test` does not leave the binary smoke.sh runs).

## Checks done
- `bash -n` on `install.sh` and `scripts/smoke.sh`: clean. shellcheck is not installed.
- README env vars, default model, Rust 1.97, CI job names, `Alt+[` bindings and the C1-C10 test claims were grepped against code and CI: consistent.
- The spike is cited by ADR 0001 and is kept. No Cargo.toml change, so `cargo check` was not needed.

## Ponytail close
Skipped: reading every ADR and every PARITY line against code; only spot checks. Did not run shellcheck (absent), install.sh or smoke.sh. Risk: none for behavior; the one edit is a doc word.
