# Patched gpui-base 0.9.1

This is the published `gpui-base` 0.9.1 (Apache-2.0, [longbridge/gpui-component](https://github.com/longbridge/gpui-component)), patched so the code editor can show a Suggestion's Replacement in the text ([ADR 0003](../../docs/adr/0003-inline-suggestions-on-patched-gpui-base.md)) and so a large document opens fast in the Reading view. The root `Cargo.toml` swaps it in with `[patch.crates-io]`, so `gpui-kit` builds against it.

To see the patch, diff this folder against the pristine crate:

```sh
diff -ru ~/.cargo/registry/src/*/gpui-base-0.9.1/src vendor/gpui-base/src
```

## What changed

Seven changes. The first is in `src/text`:

- **Reading view options** on `TextView` (`text/text_view.rs`, `text/state.rs`, `text/document.rs`), used by `crates/app/src/reading_view.rs`:
  - `lazy_measure(true)`: a scrollable view measures only the blocks it shows. The list is built without `measure_all` and `render_root` gives every block an estimated height (`estimated_block_height`), so the scrollbar thumb settles as blocks are measured instead of being exact from the first frame. Without it the first frame lays out the whole document.
  - `content_column(w, inset)`: the content is a centered column at most `w` wide with `inset` on both sides, while the view, its wheel handling and its scrollbar keep the full width of the container.
  - `scroll_padding(top, bottom)`: space above the first and below the last block that scrolls with the content (carried by the first and last block items, because the list's own padding is ignored by its scrollbar math).
  - Tests: `a_lazy_view_measures_only_the_blocks_it_shows`, `the_scrollbar_end_leaves_the_bottom_padding_below_the_last_block` and `an_eager_view_measures_every_block` in `text/state.rs`, and the estimate in `text/document.rs`.

The other six are in `src/input`:

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

- **Estimate-then-verify soft wrap**: `display_map/text_wrapper.rs`: `measured_wrap` still wraps by exact shaped widths and picks the same breaks, but finds each row's end by starting at an estimate and checking it, instead of galloping from one grapheme.
  - `largest_fitting` starts at a guess and gallops away from it, so a right guess costs two shapings (`k` fits, `k + 1` does not) and the answer is always bracketed by two measured prefixes. The guess is the previous row's width per byte, carried across lines in `update_wrapped` (`measured_wrap_hinted`); a wrong hint changes which prefixes are shaped, never the breaks. The guess is capped at twice the last row (at most 1 KB) plus 16 bytes, so a hint from zero-width text costs a bounded number of shapings, not a line's length.
  - It looks at word breaks inside a 256-grapheme window first and falls back to graphemes when every word in the window fits or the first word does not, as before. For a measure that grows with the prefix it returns what the old search returned; `text_wrapper/reference.rs` keeps the old search and `text_wrapper/fast_search_tests.rs` compares them, counts shapings per row and checks `valid_widths`, bounds the cost of bad hints and compares on the real text system (prose with tabs, no `fi`/`ff`/`fl` ligatures).
  - Shapings per row of prose go from about 15 to 2 or 3. A measure that is not monotone in the prefix (a ligature that shrinks a width) may pick a different, equally fitting break than before.

- **Rewrap only from the edit**: `display_map/text_wrapper.rs`: `rewrap_edited` and `measured_wrap_resumable`. An edit inside one line keeps the old breaks that end at least 32 bytes before it, restarts at the last of them, and stops at the first new break that lands on an old break (shifted by the edit's length change), reusing the old breaks after it. The result equals wrapping the whole line; typing in a 55 KB paragraph shaped about 400 rows per key before and now shapes a few. Lines with inline metrics, `WrappingIndent::None` and multi-line edits take the full path. Tests: `text_wrapper/incremental_tests.rs`.

- **Open a large Document with estimated rows**: `display_map/text_wrapper.rs`, `base/state.rs`, `base/element.rs`. A full wrapping pass (first layout, width, font, tab or indent change, replacing the whole text) of a Document of at least `PROGRESSIVE_MIN_BYTES` (128 KiB) shapes only the lines in `TextWrapper::focus` (three screens around the top of the viewport, a screen around the caret) and gives every other line `LineItem::estimated` rows: its length over the bytes per row seen in the focus, split evenly at character boundaries, with an empty `valid_widths` so no reuse trusts them. `LineSummary::estimated_lines` counts them.
  - Exact before use: each prepaint (`TextElement::settle_wrap`) wraps the lines near the viewport and the caret before laying out, and `scroll_to` wraps around its target. An edit wraps the lines it touches (an estimated line is never a `Prior`). Everything else that reads a line's rows (the scrollbar, the text map, the rows of a far offset) sees the estimate.
  - Convergence: `InputBaseState::start_wrap_refinement` runs `refine_wrap_slice` in 5 ms slices between frames and input (`TextWrapper::refine_lines` replaces estimated lines with the production wrap, lines after the viewport first). The result equals a full wrap: `text_wrapper/progressive_tests.rs` compares them with a fake and the native text system, and `a_large_document_opens_estimated_and_ends_with_the_rows_of_a_full_wrap` through the editor.
  - The view does not move: `InputBaseState::refine_wrap` moves the scroll offset by the rows that lines above the viewport gained or lost (`the_text_on_screen_stays_put_while_estimated_rows_become_exact`).
  - `DisplayMap::has_estimated_rows` tells a consumer that the row count is still changing; the app's text map keeps the lines it measured until the rows are exact.
- **A scroll redraws what read the layout**: `base/element.rs`, `TextElement::prepaint`. When the text moved inside the input since the last layout (any scroll, even part of a row or inside a long wrapped line), or the visible lines changed, the input asks GPUI for the next frame and notifies its state then. Whatever placed itself from the old layout in this frame (the app's Cards and its AI caret, read through `range_to_bounds`) is drawn again with the new one. Before, such a frame only notified when the caret on screen moved, and a notify made while drawing schedules no frame, so with the editor unfocused they stayed a scroll step behind until something else redrew. Test: `a_scroll_of_less_than_a_row_asks_for_the_next_frame`.

Tests for inlays and zones are in `inlays.rs`, `text_wrapper.rs`, `inline_line.rs` and at the end of `base/element.rs` (through `InputState` in a test window). The `gpui-base` CI job runs the crate's whole library suite and checks its format. From this folder:

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
