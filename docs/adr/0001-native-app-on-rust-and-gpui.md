# 0001. Native app on Rust and GPUI

- Status: Proposed (2026-11-14)
- Source: maintainer request: a lighter, faster native app without Electron, and curiosity about GPUI. Spike in [`spikes/gpui-editor/`](../../spikes/gpui-editor/). Links named "desafinado NNNN" point to [the ADRs of the Electron and Web app](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/).

## Context

The Electron build ([desafinado 0013](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0013-native-app-on-electron.md)) ships Chromium: the package is 100–200 MB and the app uses more memory than a browser tab. The maintainer wants a native app built on GPUI, the UI framework from Zed. It runs alongside the Electron and Web builds; it does not replace them yet. Linux first, as with Electron.

GPUI has no stable API, and upstream publishes rarely (`gpui` 0.2.2 on crates.io). Zed's own editor is not reusable outside Zed. The ecosystem around it as of November 2026:

- **[gpui-kit](https://gpui-kit.com/)** 0.9.1 (Longbridge, formerly gpui-component). One crate that pins a matching set of `gpui-pre` crates (`=0.3.12`) and adds `gpui-base` and a styled component library. It has a code editor: rope buffer, tree-sitter highlighting (`tree-sitter-markdown`), soft wrap, folding, undo, search, and Monaco-style decorations.
- **[Velotype](https://github.com/manyougz/velotype)** (Apache 2.0, `gpui` 0.2): a block WYSIWYG Markdown editor with its own document model. [gpui-markdeditor](https://github.com/yuWorm/gpui-markdeditor) extracts its editing core.
- **[gpui-kit-markdown-editor](https://github.com/wabzqem/gpui-kit-markdown-editor)**: a block WYSIWYG editor on gpui-kit's `editor_host` API (merged October 2026).

Desafinado edits raw Markdown with overlaid Suggestions ([desafinado 0001](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0001-inline-overlay-suggestions.md)). The WYSIWYG editors do not fit that, so the spike tried gpui-kit's code editor.

## What the spike found

Run on Linux under Xvfb with X11 and llvmpipe (software Vulkan), release build, gpui-kit 0.9.1, Rust 1.97.

| Check | Result |
|---|---|
| Open a `.md` file, Markdown highlighting, soft wrap | Works with `EditorState::new(..).language("markdown").soft_wrap(true)`. |
| Strike through the Original | Works: a `TextDecoration` with `HighlightStyle { strikethrough, color }`. Decorations follow edits. |
| Mark the Original's range | Works: a `RangeDecoration` with `RangeDecorationStyle::Fill`. |
| Show the Replacement next to the Original | **Gap.** No inline widget or block that reserves space between lines. The spike draws a card under the range with `range_to_bounds`, and it covers the text below (screenshot 1). |
| Accept as a normal, undoable edit | Works: `set_selected_range` and then `replace`. `Ctrl+Z` restores the Original. |
| Reject | Works: clear both decoration collections. |
| Bigger headings ([DESIGN.md](https://github.com/walteraandrade/desafinado/blob/main/DESIGN.md) Headline at 1.85em) | **Gap.** `HighlightStyle` has no font size, and the code editor uses one row height. Headings can be bold and colored, not bigger. |
| Body font | The code editor is monospace by default; the font family can be set on the element. |
| First frame | 233 ms for PRODUCT.md, 115 ms for a 476 KB, 7,600-line file (warm cache). |
| Memory (RSS) | 176 MB for PRODUCT.md, 201 MB for the 476 KB file, 220 MB after typing. llvmpipe inflates this; it needs a re-run on a real GPU. |
| Binary | 60 MB release, 43 MB stripped, all statically linked. |
| Clean release build | about 6 minutes on 4 cores, 806 crates. |
| Not checked | Wayland, IME, a real GPU, HiDPI. They need the maintainer's machine. |

## Decision (proposed)

- **Where it lives:** its own repository, `desafinado-native`, a Cargo workspace. It follows Desafinado's GLOSSARY, PRODUCT and DESIGN, and copies the prompts and test fixtures it needs. The Electron and Web builds in `desafinado` are not touched.
- **Editor:** gpui-kit's code editor, pinned to an exact version (`=0.9.1`), with GPUI only through gpui-kit, so a GPUI bump is one line and a re-run of the checks above.
- **Suggestions:** decorations for the Original. For the Replacement, a card under the range until gpui-kit has view zones, Monaco's API for blocks of space between lines, which gpui-kit already models its decorations on. We propose view zones upstream. If upstream declines, the fallback is a fork of the input module, not a new editor. The card is a change to [desafinado 0001](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0001-inline-overlay-suggestions.md)'s "the Replacement shows next to it", accepted only for the first milestone.
- **Headings:** bold and colored, same size, until the editor supports per-range font size. A change to DESIGN.md's Headline for this build only.
- **Colors:** the theme's solid roles, as in [desafinado 0017](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0017-book-themes-and-solid-colors.md). The spike's 12% fill is an alpha color and does not carry over.
- **OpenRouter:** called directly from Rust, no proxy. GPUI's `HttpClient` trait, implemented with `reqwest` on a small Tokio runtime, the way Zed does it. Streaming is SSE parsed by the app; structured output uses `response_format` with a JSON Schema, validated with `serde` before a Suggestion is shown ([desafinado 0002](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0002-structured-output-app-owned-anchors.md), [desafinado 0008](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0008-validated-streaming-only.md)). The AI SDK ([desafinado 0009](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0009-ai-sdk-v7.md)) has no Rust equivalent, so the AI call records are written by the client itself.
- **Key:** stored in the Secret Service (libsecret) through the `keyring` crate, the same backend the Electron build pins with `--password-store=gnome-libsecret`. With no Secret Service, the same choice as 0013: session only, or plain text in a 0600 file if the maintainer agrees.
- **Verification:** `agent-browser` cannot drive a GPUI window. Checks run under Xvfb with `xdotool` and screenshots; Wayland and IME checks run on the maintainer's machine.
- **First milestone:** open and save Markdown, the editor, Mood Indigo, the key screen, and one quick action (Shorter on a selected paragraph) that streams one Suggestion to Accept or Reject. The pure modules (diff to hunks, Suggestion state and position mapping, Block extraction, prompt building, schema validation) are ported first, test first, reusing the TypeScript test cases.

## Consequences

- A second implementation of the same product in a second repository. Prompts and test fixtures are copied, so a change in `desafinado` has to be carried over by hand.
- The native build depends on a third-party crate that tracks an unstable framework. Pinning exact versions keeps builds reproducible; bumps are deliberate.
- Until view zones exist, the Replacement can cover text below it, so the card must move or collapse while the writer scrolls or types.
- Release builds are slow (minutes); CI needs a cargo cache.

## Alternatives considered

- **Velotype or gpui-kit-markdown-editor:** WYSIWYG block editors with their own document model. Desafinado's Document is a plain Markdown string ([desafinado 0001](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0001-inline-overlay-suggestions.md)); a block model brings back the round-trip problem desafinado 0001 rejected.
- **An editor written from scratch on GPUI primitives:** full control over inline blocks and heading sizes, but months of work (selection, IME, undo, wrapping, search) before the first Suggestion.
- **Zed's `editor` crate:** has inlays and blocks, but it is tied to Zed's workspace, project and language crates and is not published.
- **Tauri:** drops Chromium only partly (WebKitGTK on Linux) and keeps the web stack; it does not answer the maintainer's interest in GPUI.
