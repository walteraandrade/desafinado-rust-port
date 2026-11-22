# Patched gpui-base 0.9.1

This is the published `gpui-base` 0.9.1 (Apache-2.0, [longbridge/gpui-component](https://github.com/longbridge/gpui-component)), patched so the code editor can show a Suggestion's Replacement in the text ([ADR 0003](../../docs/adr/0003-inline-suggestions-on-patched-gpui-base.md)). The root `Cargo.toml` swaps it in with `[patch.crates-io]`, so `gpui-kit` builds against it.

To see the patch, diff this folder against the pristine crate:

```sh
diff -ru ~/.cargo/registry/src/*/gpui-base-0.9.1/src vendor/gpui-base/src
```

## What changed

Two features, both in `src/input`:

- **Inlays**: text drawn inside a line that is not in the buffer. `InputState::set_inlays(Vec<Inlay>)` takes an offset, the text and a `HighlightStyle`.
  - `display_map/inlays.rs` (new): `Inlay`, `BlockZone`, `shift_offset`.
  - `base/element.rs`: `measure_inlays` measures each inlay with the grapheme it sits next to and feeds the width to the wrapper as an inline metric, so wrapping makes room for it. `shape_inlay_row` shapes a row that holds an inlay as an `InputLine::inline` of text and inlay fragments.
  - `display_map/inline_line.rs`: a click on an inlay lands at its offset, and the caret at that offset sits before it, where typed text goes. An inlay's background color counts toward its line's background.
  - `display_map/display_map.rs`: inlays move with edits before them and drop when an edit covers them.
- **Block zones**: empty rows reserved after a line, like Monaco's view zones. `InputState::set_block_zones(Vec<BlockZone>)` takes an offset and a row count; the app draws its own element over the reserved rows.
  - `display_map/text_wrapper.rs`: `LineItem::zone_rows` counts in `lines_len()` (the wrap rows) but not in `text_rows()`. Offsets never map into a zone; a zone row maps to the end of its line. `LineLayout::zone_rows` adds the height. Zones move with edits like inlays.
  - `display_map/wrap_map.rs`, `display_map/display_map.rs`, `display_map/fold_map.rs`: `set_zones` and `skip_zone_row`; a zone or inline width change rebuilds the fold projection even when the row count stays the same.
  - `base/movement.rs`: up and down skip the zone rows, counting in display rows so folds are honored.
  - `base/element.rs`, `base/state.rs`, `editor/indent.rs`: selection, folds, line numbers, clicks and indent guides use the text height where they used the line height.

Tests for both are in `inlays.rs`, `text_wrapper.rs`, `inline_line.rs` and at the end of `base/element.rs` (through `InputState` in a test window). The `gpui-base` CI job runs the crate's whole library suite and checks its format. From this folder:

```sh
cargo fmt --check
cargo test --features test-support --lib
```

Known limits:

- An inlay on an empty line, or past the end of the text, takes no width in the wrapping.
- An inlay is measured in the editor's font without its own style, so a bold or italic inlay can run a little past the width reserved for it.
- `set_inlays` trusts its offsets: one inside a multi-byte character or text holding a line break is a caller bug.
- A selection or fill that spans a block zone draws one outline through the zone rows; the app's zone element covers it.
- Inline tokens and inlays share the inline metrics. While inlays exist the token path leaves them alone; an editor that shows both at once is not supported.
- The CI job builds with this folder's own `Cargo.lock`, which can drift from the root lock the app builds with.

## On a gpui-kit bump

The patch has to be applied again to the new `gpui-base`: copy the new crate here, re-apply the diff above, run the tests, and check a Suggestion in the window (a word change, a block change and a failed Suggestion). If upstream has shipped view zones or inlays by then, drop this folder and use theirs.
