# 0003. Inline Suggestions on a patched gpui-base

- Status: Accepted (2026-11-22)
- Source: maintainer request. The Replacement showed in a Card, in the margin or floating over the text, which read as a modal rather than a diff. Electron shows it in the text ([desafinado 0001](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0001-inline-overlay-suggestions.md)). [ADR 0001](0001-native-app-on-rust-and-gpui.md) accepted the Card for the first milestone only, with a fork of the input module as the fallback if gpui-kit did not gain view zones.

## Context

A Suggestion needs two things the gpui-kit code editor (0.9.1) does not have:

- **Text inside a line that is not in the buffer**, for a word-level Replacement next to the struck-through Original.
- **Space between lines**, for a block-level Replacement or a failed Suggestion's error, with text below pushed down.

What else was looked at:

| Option | Why not |
|---|---|
| gpui-kit 0.10 | No view zones or inlays either. Its input module has ghost text for completions (one at a time, at the cursor), fold placeholders (replace text, cannot sit next to it) and token rendering (draws the buffer's own text). None holds a Replacement. |
| Upstream first | ADR 0001's plan. Still worth doing, but it would block the feature for an unknown time. The patch below is the same shape as Monaco's API, so it can be offered upstream as is. |
| gpui-monaco | Has view zones and inlays, but pins `gpui-pre` 0.3.8 against our 0.3.12, is about 195k lines, targets macOS first, and would mean moving every editor feature (decorations, Find and replace, IME, folds, the Linux setup) to a new editor. |
| Zed's editor | Has both (blocks and inlays) but is not published as a crate and depends on Zed's workspace. |
| Simulate in the buffer | Insert the Replacement as real text and hide it from save and undo. Breaks desafinado 0001's rule that the Document holds only the writer's text, and every buffer consumer (save, History, Find, Block extraction) would need to skip it. |
| A second editor below the line | An absolute element over the text cannot push the lines below down; it covers them. That is what the Card did. |

## Decision

1. **Vendor `gpui-base` 0.9.1 in `vendor/gpui-base`** and swap it in with `[patch.crates-io]` in the root `Cargo.toml`. It is the crate that holds gpui-kit's input module. `vendor/gpui-base/PATCH.md` lists the changes and how to re-apply them.
2. **Inlays**: text at an offset, drawn in the line with its own style. The wrapper reserves its width the way it does for inline elements, so wrapping and clicks stay right. A word-level Suggestion (`AdaptiveDiff` in Word mode) shows each inserted hunk as an inlay in the AI color on `ai_fill`, after the struck-through deletion, with a thin space between them.
3. **Block zones**: rows reserved after a line, as in Monaco's view zones. Up and down skip them, offsets never land in them, and the app draws its element over them. A block-level Suggestion shows its Replacement in a zone under the last line of its range (`ui::replacement_block`); a failed Suggestion shows its error, Retry and Dismiss there too. The app measures the element and asks for as many rows as it needs.
4. **The Card keeps the rest**: versions, Rationale, Changes and the actions. It no longer shows the Replacement.
5. **Offsets in the adaptive diff are fixed**: `join_changes` dropped the separator from the deleted side when it joined an insertion with the change after it, so later hunks sat one character early. Invisible while the Replacement lived in the Card, wrong once inlays sit at those offsets. The Electron app has the same bug in `joinChanges`.

## Consequences

- Each gpui-kit bump has to re-apply the patch; `PATCH.md` says how. The patch touches the wrapper, the display map, the element, movement and state, about 500 changed lines.
- The vendored crate's own tests are not in the workspace (`exclude`), so `cargo test --workspace` does not run them; the `gpui-base` CI job does.
- Zone height is measured after the first draw, so a new zone may show for a frame at its default height (two rows) before it fits its content.
- Offering inlays and block zones upstream remains open. If gpui-kit ships them, this folder goes away.
