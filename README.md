# Desafinado Rust port

A native Linux build of [Desafinado](https://github.com/walteraandrade/desafinado), the Markdown editor with an AI co-author, written in Rust on GPUI. It runs alongside the Electron and Web builds and does not replace them.

Status: spike done, decisions proposed in [ADR 0001](docs/adr/0001-native-app-on-rust-and-gpui.md).

## Plan

1. Spike and ADR: [`spikes/gpui-editor/`](spikes/gpui-editor/) and ADR 0001.
2. Walking skeleton: a Cargo workspace with a window, Open and Save Markdown, Markdown highlighting, Mood Indigo, the key screen, and CI with `cargo test` and clippy.
3. One Suggestion end to end: port the pure modules test first (diff to hunks, Suggestion state and position mapping, Block extraction, prompt building, schema validation), then Shorter on a selected paragraph, streamed from OpenRouter, to Accept or Reject.
4. Measure startup, memory and package size against the Electron build, then pick the next slices.

## Install

Needs a Rust toolchain (edition 2024, so Rust 1.85 or newer) and the usual GPUI build dependencies on Linux (`libxkbcommon`, Wayland and X11 client libraries, Vulkan loader, `fontconfig`).

```bash
./install.sh
```

The script runs `cargo install --locked --path crates/app` and puts a `desafinado` binary in `~/.cargo/bin` (or `$CARGO_INSTALL_ROOT/bin`). Then it writes a **Desafinado (native)** desktop entry (`desafinado-native.desktop`, `MimeType=text/markdown;text/x-markdown;`, `Exec=<absolute path> %f`) and its icon under `${XDG_DATA_HOME:-~/.local/share}`. Fonts, icons and themes are embedded in the binary, so it runs from any working directory. The Electron build's own desktop entry and window class (`desafinado`) are left alone; the native window uses the class `desafinado-native`.

| Option | Effect |
|---|---|
| `--root DIR` | cargo install root, so the binary lands in `DIR/bin` |
| `--debug` | build the dev profile instead of release |
| `--skip-build` | only write the desktop entry and icons for a binary already installed at the root |
| `--uninstall` | remove the binary, the desktop entry and the icons; settings and the key stay |

Without the script, `cargo install --path crates/app` alone installs the binary.

## Run

```bash
desafinado                 # opens the last file, or a new document
desafinado notes/today.md  # opens a file (relative to the current directory)
```

A second launch hands the file to the running window instead of opening another one.

From the checkout, `cargo run -p desafinado -- <file>` does the same.

## User data

Settings, Document History, Quick actions, Library roots, Skill eval runs, the model choice (`model.txt`) and AI call records (`ai-calls.jsonl`) live in `~/.config/desafinado-native` (`$XDG_CONFIG_HOME/desafinado-native`). These are kept apart from the Electron build's `~/.config/desafinado`. The OpenRouter key goes to the Secret Service keyring (service `desafinado-native`) and falls back to `openrouter-key.json` in that folder.

## Environment variables

All optional.

| Variable | Effect |
|---|---|
| `DESAFINADO_USER_DATA_DIR` | Absolute path to use instead of `~/.config/desafinado-native`. Always set it to a fresh temp dir for checks. |
| `DESAFINADO_MODEL` | OpenRouter model id used when no model was picked in the app (`model.txt` wins). |
| `DESAFINADO_FAKE_AI` | Any value: a scripted local model instead of OpenRouter, so no network and no cost. It writes AI call records with model `fake/desafinado`. Prompt markers script failures: `fake:slow`, `fake:network`, `fake:invalid`, `fake:credit`, `fake:partial`. |
| `DESAFINADO_CLAUDE_CLI` | Path to the `claude` CLI for Skill evals, when it is not on `PATH` or at `~/.local/bin/claude`. |
| `DESAFINADO_DEV_PICK_FILE` | Absolute path of a text file. Open, Save as and the Library's Add skip the native dialog and use the path written in that file, read at each dialog. For scripted checks. |
| `DESAFINADO_OPENROUTER_BASE_URL` | Another OpenRouter-compatible base URL, for tests against a local stub. |
| `HTTPS_PROXY`, `SSL_CERT_FILE` | OpenRouter calls go through `HTTPS_PROXY` when it is set, and trust the extra CA certificates in `SSL_CERT_FILE` on top of the bundled roots. That is how the app runs behind oneCLI: start it from a shell where oneCLI exported both. |

Product names follow Desafinado's [GLOSSARY](https://github.com/walteraandrade/desafinado/blob/main/GLOSSARY.md), [PRODUCT](https://github.com/walteraandrade/desafinado/blob/main/PRODUCT.md) and [DESIGN](https://github.com/walteraandrade/desafinado/blob/main/DESIGN.md).
