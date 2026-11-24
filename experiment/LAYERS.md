# Layers

Eleven layers. Every tracked file belongs to exactly one. Files under `experiment/` are not tracked source; the lead owns them.

Crate graph today (from each `Cargo.toml`): `core` ← `ai` ← `platform` ← `runtime` ← `app`; `features` depends on no internal crate; `platform` and `app` depend on `features`. A layer "may depend on" the layers below it in that graph and nothing else.

## 01 core

- **Purpose:** pure domain of the editor: Blocks from the Markdown tree, the adaptive diff, Suggestion state, position mapping, `log_err`.
- **Owns:** `crates/core/**`
- **May depend on:** external crates only.
- **Why here:** it is the bottom of the crate graph; everything else imports it, so its public API is the most expensive to change and gets its own reviewer.

## 02 ai

- **Purpose:** prompts, schemas, stream parsing, AI call records, and the editor's pure UI rules (`editor/`, `ui/`).
- **Owns:** `crates/ai/**`
- **May depend on:** 01 core.
- **Why here:** one crate, one `Cargo.toml`. The `editor/` and `ui/` modules are GPUI-free rules that live in this crate today; splitting them into another layer would make two reviewers share `lib.rs` and `Cargo.toml`.

## 03 features-skills

- **Purpose:** Skills: frontmatter parsing and editing, YAML, the Library tree and search, new-Skill templates.
- **Owns:** `crates/features/src/skills/**`
- **May depend on:** external crates; other `features` modules only through their current public API.
- **Why here:** at ~9.5k lines it is almost half of `features` and has its own clear seam (the `skills` module).

## 04 features-domain

- **Purpose:** the rest of `features`: History, Evals logic, spell checker, themes, and the small shared helpers.
- **Owns:** `crates/features/Cargo.toml`, `crates/features/src/{lib.rs,caret.rs,diff.rs,text.rs}`, `crates/features/src/{history,evals,spelling,theme}/**`, `crates/features/dictionaries/**`, `crates/features/tests/**`
- **May depend on:** external crates; 03 through its public API.
- **Why here:** the crate root (`lib.rs`, `Cargo.toml`) must have one owner; this layer takes it because it holds the shared helpers.

## 05 platform

- **Purpose:** I/O: files and atomic save, keyring, OpenRouter client, SQLite history, the watcher, Evals runner, user data.
- **Owns:** `crates/platform/**`
- **May depend on:** 01, 02, 03, 04.
- **Why here:** everything that touches disk, network or processes. Known lead: `platform/src/history/{thin,word_delta,types}.rs` look like copies of `features/src/history/` ones.

## 06 runtime

- **Purpose:** wires services and the AI backend; the `ai-check` binary.
- **Owns:** `crates/runtime/**`
- **May depend on:** 01, 02, 05.
- **Why here:** small, but it is the composition root below the UI; mixing it with platform would hide wiring code among I/O code.

## 07 app-shell

- **Purpose:** the GPUI process and window frame: startup, workspace, styling (`ui.rs`, `theme.rs`), menus, logging, single instance, the UI rules test.
- **Owns:** `crates/app/Cargo.toml`, `crates/app/build.rs`, `crates/app/assets/**`, `crates/app/tests/**`, `crates/app/src/{main,workspace,ui,theme,logger,single_instance,start_error,last_file,menu_nav}.rs`
- **May depend on:** 01–06, GPUI.
- **Why here:** `ui.rs` is the single styling module AGENTS.md demands and `workspace.rs` owns the window; both are shared by every view, so one owner keeps them coherent.

## 08 app-editor

- **Purpose:** the text surface: the editor pane, document, caret, Suggestions in the text, Review comments, reading view, text map, spelling marks, the requests tray, the frontmatter panel.
- **Owns:** `crates/app/src/{editor_pane,document,caret,comment_review,editor_place,reading_view,text_map_view,spelling,jobs_tray,frontmatter_view}.rs`
- **May depend on:** 01–06, 07's public items, GPUI.
- **Why here:** these views all render or react to the open document's text.

## 09 app-panels

- **Purpose:** every panel and dialog around the editor: Library, Evals, History, Preferences, key and model dialogs, new Skill, AI spend.
- **Owns:** `crates/app/src/{library_panel,evals_view,eval_run_dialog,new_skill_dialog,history_view,history_store,preferences_view,key_dialog,model_dialog,spend,spend_view}.rs`
- **May depend on:** 01–06, 07's public items, GPUI.
- **Why here:** these views sit beside or over the editor and talk to services, not to the text.

## 10 vendor-gpui-base

- **Purpose:** gpui-kit's `gpui-base`, patched with inlays and block zones (ADR 0003).
- **Owns:** `vendor/gpui-base/**`
- **May depend on:** its own upstream dependencies.
- **Why here:** third-party code with a documented patch. Only the patch (see `PATCH.md`) is ours; the rest must stay byte-close to upstream so a gpui-kit bump can re-apply it.

## 11 tooling-docs

- **Purpose:** workspace config, CI, scripts, install, packaging, docs, the old spike.
- **Owns:** `Cargo.toml`, `Cargo.lock`, `clippy.toml`, `deny.toml`, `.gitignore`, `.sonarcloud.properties`, `.github/**`, `AGENTS.md`, `CLAUDE.md`, `README.md`, `install.sh`, `scripts/**`, `packaging/**`, `docs/**`, `spikes/**`
- **May depend on:** nothing; it describes and builds the others.
- **Why here:** none of these files is compiled into the app; they change for different reasons than code.

## Rejected alternatives

One layer per crate (6 layers) was rejected because `app` (~20k lines) and `features` (~21k) are too big for one reviewer each, while `runtime` is tiny. Splitting by concern across crates (an "AI" layer with `ai`, `runtime/openrouter_ai.rs` and `platform/openrouter`) was rejected because two layers would then share a crate's `lib.rs` and `Cargo.toml`, and a file-ownership rule cannot hold. Folding `vendor/gpui-base` into tooling was rejected because its rule ("touch only the patch") is unlike any other layer's.
