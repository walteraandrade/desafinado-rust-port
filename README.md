# Desafinado (native)

[![CI](https://github.com/walteraandrade/desafinado-rust-port/actions/workflows/ci.yml/badge.svg)](https://github.com/walteraandrade/desafinado-rust-port/actions/workflows/ci.yml)

A Markdown editor with an AI co-author, as a native Linux app written in Rust on [GPUI](https://www.gpui.rs), the UI framework behind Zed.

This is the native build of [Desafinado](https://github.com/walteraandrade/desafinado). The Electron and Web builds live in that repository and keep working; this one runs next to them, with its own settings, its own desktop entry and no Chromium. It starts faster, uses less memory, and installs as a single binary.

The AI proposes; your voice wins. Every change it suggests sits in your text as a tracked change until you accept it.

## What it does

**Write with the AI.**

- Select text, or put the cursor in a paragraph, and press `Ctrl+K`. Pick a Quick action (Shorter, Clearer, Fix grammar, or one of your own) or type any instruction.
- The answer shows in place: the old text struck through, the new text and a one-line rationale on a card in the margin. Accept (`Ctrl+Enter`), Reject (`Esc`), or Refine it ("shorter", then "now more formal") and step between versions with `Ctrl+H` and `Ctrl+L`.
- Accept is a normal edit, so `Ctrl+Z` undoes it.
- **Edit → Review…** reads the whole document, optionally with a focus such as "tone only", and streams up to six suggestions, with Accept all and Reject all.

**Read it rendered.** `Ctrl+Alt+R` (or **Edit → Reading view**) swaps the editor for the rendered Markdown: real heading sizes, lists, quotes, code blocks and links, without the frontmatter. Press it again to edit.

**Keep the history.** The timeline under the header keeps a version after each accept, open, restore, save and pause in your typing, marked by who made it. Compare any version with the current text and restore it. Each document keeps its own history, with no cap, in a SQLite database in the settings folder.

**Edit agent Skills.** The Library (`Ctrl+B`) lists the Skills in `~/.claude/skills`, `~/.agents/skills` and any folder you add, by their frontmatter name. Library search (`Ctrl+P`) finds Skills and files as you type. Files on disk are the source of truth: saves are atomic, and when an agent or `git pull` changes the open file, the editor reloads it, or asks first if you have unsaved text. The Evals page runs a Skill's evals through the Claude CLI, with and without the Skill, and shows whether it helped.

**See what the AI costs.** **File → AI spend…** shows total spend, calls, tokens, failed and retried calls, a 14-day chart, cost by model and by task, and the latest calls, for today, 7 days, 30 days or all time. Every call is also appended to `ai-calls.jsonl` in the settings folder.

The app talks to [OpenRouter](https://openrouter.ai) directly. You bring your own API key.

## Status

Every feature of the Electron build works here; [docs/PARITY.md](docs/PARITY.md) lists them one by one, with each place the native build behaves differently. The main differences come from GPUI:

- Headings are bold but keep the body text size, and code uses the body font.
- The suggested text shows in the margin card rather than inline between lines.
- Most motion (caret glide, fades, word-by-word reveal) is not there yet.

Linux only. There is no distribution package yet: you build and install it from this repository. The decision to build on GPUI is recorded in [ADR 0001](docs/adr/0001-native-app-on-rust-and-gpui.md).

## Install

You need a Rust toolchain and a few system libraries.

**Rust.** Install it with [rustup](https://rustup.rs). The workspace uses edition 2024 (Rust 1.85 or newer); CI builds with Rust 1.97.

**System libraries.** GPUI draws with Vulkan and talks to Wayland or X11 directly.

```sh
# Arch
sudo pacman -S --needed base-devel libxkbcommon libxkbcommon-x11 wayland libxcb \
  vulkan-icd-loader fontconfig freetype2

# Debian, Ubuntu
sudo apt install build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libx11-xcb-dev libxcb1-dev libvulkan-dev libfontconfig-dev libfreetype-dev

# Fedora
sudo dnf install gcc pkgconf-pkg-config libxkbcommon-devel libxkbcommon-x11-devel \
  wayland-devel libxcb-devel vulkan-loader-devel fontconfig-devel freetype-devel
```

You also need a Vulkan driver for your GPU (Mesa's `vulkan-radeon` or `vulkan-intel` on Arch, `mesa-vulkan-drivers` on Debian and Ubuntu, or NVIDIA's own). `vulkaninfo --summary` tells you whether one is working.

**Build and install.**

```sh
git clone https://github.com/walteraandrade/desafinado-rust-port.git
cd desafinado-rust-port
./install.sh
```

The release build takes a few minutes the first time. The script installs the `desafinado` binary in `~/.cargo/bin` and adds a **Desafinado (native)** entry to your app launcher, registered for Markdown files. Fonts, icons and themes are inside the binary, so you can move or delete the checkout afterwards.

| Option | Effect |
|---|---|
| `--root DIR` | install the binary in `DIR/bin` instead of `~/.cargo/bin` |
| `--debug` | build the debug profile (faster to build, slower to run) |
| `--skip-build` | only write the desktop entry and icons for a binary already installed |
| `--uninstall` | remove the binary, the desktop entry and the icons; settings, history and the key stay |

To make it your default app for Markdown:

```sh
xdg-mime default desafinado-native.desktop text/markdown text/x-markdown
```

The Electron build's desktop entry and window class (`desafinado`) are left alone. The native window uses the class `desafinado-native`, so the two never group together.

## First run

```sh
desafinado                 # reopens the last file, or starts a new document
desafinado notes/today.md  # opens a file
```

Opening a second file while the app runs hands it to the open window.

Without an OpenRouter key the app still opens, with a banner that says the key is missing. Click it and paste your key once; it goes to your system keyring (Secret Service, as GNOME Keyring or KWallet provide), or, if there is no keyring, to a file in the settings folder after the app asks you. You can change or remove it later in **Preferences** (`Ctrl+,`), where you also pick the model, the theme and your Quick actions. The default model is `openai/gpt-6.1-terra`.

If your machine reaches the internet through a proxy, such as oneCLI, start the app from a shell where `HTTPS_PROXY` (and `SSL_CERT_FILE`, for the proxy's certificate) are set. AI calls go through it.

## Keys

| Keys | Action |
|---|---|
| `Ctrl+N` `Ctrl+O` `Ctrl+S` `Ctrl+Shift+S` | New, Open, Save, Save as |
| `Ctrl+K` | Ask the AI about the selection or the paragraph under the cursor |
| `Ctrl+Enter` / `Esc` | Accept / Reject the suggestion at the cursor |
| `Tab` | Move into the suggestion's card |
| `Ctrl+H` / `Ctrl+L` | Previous / next version of a refined suggestion |
| `Alt+[` / `Alt+]` | Previous / next suggestion |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / Redo |
| `Ctrl+B` / `Ctrl+P` | Library / Library search |
| `Ctrl+,` | Preferences |
| `Alt+F` / `Alt+E` / `F10` | Open the File or Edit menu |

## Where your data lives

Everything is in `~/.config/desafinado-native` (or `$XDG_CONFIG_HOME/desafinado-native`), kept apart from the Electron build's `~/.config/desafinado`:

| File | What |
|---|---|
| `history.sqlite` | Document history |
| `ai-calls.jsonl` | One record per AI call: task, model, provider, tokens, cost, latency |
| `model.txt`, `theme.txt` | Model and theme picked in Preferences |
| `quick-actions.json` | Your Quick actions |
| `openrouter-key.json` | The key, only when there is no system keyring |

Unsaved text, Library folders and Eval runs are kept there too.

## Development

```sh
cargo run -p desafinado -- notes.md        # run from the checkout
DESAFINADO_FAKE_AI=1 cargo run -p desafinado  # with a scripted local model: no network, no cost
cargo test --workspace                     # every test; none calls the model
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI runs those three checks on every pull request and push to `main`, builds the app and starts it off-screen under Xvfb ([`scripts/smoke.sh`](scripts/smoke.sh)). A pull request should pass the `checks` job before it merges.

To check a change against the real model, `cargo run -p desafinado-runtime --bin ai-check` sends one suggestion and one review pass to OpenRouter and prints what they cost. It spends real money (a fraction of a cent).

**Workspace.** Logic lives in plain Rust crates with unit tests; only `app` knows about GPUI.

| Crate | What it holds |
|---|---|
| [`core`](crates/core) | Blocks from the Markdown tree, the adaptive diff, suggestion state and position mapping |
| [`ai`](crates/ai) | Prompts, response schemas and stream parsing, AI call records, and the editor's pure UI rules (caret, bubble, menus) |
| [`features`](crates/features) | History, Skills, Evals and themes |
| [`platform`](crates/platform) | Files and atomic save, the keyring, the OpenRouter client, SQLite history, the Library watcher, user data |
| [`runtime`](crates/runtime) | Wires the services and the AI backend together; the `ai-check` binary |
| [`app`](crates/app) | The GPUI window and every view; the `desafinado` binary |

[`spikes/gpui-editor`](spikes/gpui-editor) is the throwaway spike behind ADR 0001 and is not part of the workspace.

**Environment variables.** All optional; mostly for development and scripted checks.

| Variable | Effect |
|---|---|
| `DESAFINADO_USER_DATA_DIR` | Use this folder instead of `~/.config/desafinado-native`. Point it at a fresh temporary folder for checks. |
| `DESAFINADO_MODEL` | OpenRouter model to use when none was picked in Preferences. |
| `DESAFINADO_FAKE_AI` | Any value: a scripted local model instead of OpenRouter. Its records use the model `fake/desafinado`. Put `fake:slow`, `fake:network`, `fake:invalid`, `fake:credit` or `fake:partial` in an instruction to script a failure. |
| `DESAFINADO_CLAUDE_CLI` | Path to the `claude` CLI for Evals, when it is not on `PATH` or in `~/.local/bin`. [`scripts/fake-claude`](scripts/fake-claude) stands in for it without calling any model. |
| `DESAFINADO_DEV_PICK_FILE` | Path of a text file. Open, Save as and the Library's Add skip the file dialog and use the path written in it, read each time. For scripted checks. |
| `DESAFINADO_OPENROUTER_BASE_URL` | Another OpenRouter-compatible base URL, such as a local stub. |
| `DESAFINADO_COMMIT` | The commit shown in Preferences, for builds outside a git checkout. |
| `HTTPS_PROXY`, `SSL_CERT_FILE` | Proxy for OpenRouter calls, and extra CA certificates trusted on top of the bundled ones. |

The product's vocabulary (Suggestion, Block, Review pass, Version and so on) follows Desafinado's [GLOSSARY](https://github.com/walteraandrade/desafinado/blob/main/GLOSSARY.md), and its look and behavior follow [PRODUCT](https://github.com/walteraandrade/desafinado/blob/main/PRODUCT.md) and [DESIGN](https://github.com/walteraandrade/desafinado/blob/main/DESIGN.md).
