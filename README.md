# Desafinado Native

A native Linux build of [Desafinado](https://github.com/walteraandrade/desafinado), the Markdown editor with an AI co-author, written in Rust on GPUI. It runs alongside the Electron and Web builds and does not replace them.

Status: spike done, decisions proposed in [ADR 0001](docs/adr/0001-native-app-on-rust-and-gpui.md).

## Plan

1. Spike and ADR: [`spikes/gpui-editor/`](spikes/gpui-editor/) and ADR 0001.
2. Walking skeleton: a Cargo workspace with a window, Open and Save Markdown, Markdown highlighting, Mood Indigo, the key screen, and CI with `cargo test` and clippy.
3. One Suggestion end to end: port the pure modules test first (diff to hunks, Suggestion state and position mapping, Block extraction, prompt building, schema validation), then Shorter on a selected paragraph, streamed from OpenRouter, to Accept or Reject.
4. Measure startup, memory and package size against the Electron build, then pick the next slices.

Product names follow Desafinado's [GLOSSARY](https://github.com/walteraandrade/desafinado/blob/main/GLOSSARY.md), [PRODUCT](https://github.com/walteraandrade/desafinado/blob/main/PRODUCT.md) and [DESIGN](https://github.com/walteraandrade/desafinado/blob/main/DESIGN.md).
