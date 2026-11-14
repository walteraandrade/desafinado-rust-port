# GPUI editor spike

Throwaway code for [ADR 0001](../../docs/adr/0001-native-app-on-rust-and-gpui.md). It opens a Markdown file in gpui-kit's code editor and overlays one fake Suggestion on the second paragraph: the Original is struck through, and the Replacement shows in a card with Accept and Reject.

Linux needs `libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libx11-xcb-dev` (Ubuntu names) and a Vulkan driver.

```bash
cargo run --release -- /path/to/file.md
```

Off-screen: `env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 xvfb-run -a cargo run --release -- /path/to/file.md`.

Accept and Reject are buttons on the card; `Ctrl+Z` in the editor undoes an Accept. The spike also binds `Ctrl+Enter`, `Ctrl+Backspace` and `Ctrl+S`, but they only fire when the spike view has focus, which it does not take yet.
