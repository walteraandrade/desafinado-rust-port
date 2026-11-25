# AGENTS.md

Desafinado (native): a Markdown editor with an AI co-author, in Rust on GPUI, Linux only. The Electron app in [walteraandrade/desafinado](https://github.com/walteraandrade/desafinado) is the reference for behavior and names; its AGENTS.md (pnpm, Vite, Biome) does not apply here.

These are traps to avoid, not a map of the code. The map is the README's Workspace table. Add a rule here when the same mistake shows up a second time.

## Before you finish

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace            # about 3 minutes; none calls the model
```

CI also builds the app and starts it off-screen with [`scripts/smoke.sh`](scripts/smoke.sh). To see a UI change, run it under Xvfb with `DESAFINADO_FAKE_AI=1` and a fresh `DESAFINADO_USER_DATA_DIR` (both in the README). Tests and checks never call OpenRouter; `ai-check` does and costs money, so run it only when asked.

## Rust

- Lints live in `[workspace.lints]` in the root `Cargo.toml`, clippy pedantic included. Fix the code; when a lint is wrong for one item, use `#[expect(lint, reason = "...")]`, never a bare `#[allow]`.
- Never drop a `Result` with `let _ =`. Use `?`, a `match`, or `desafinado_core::log_err::LogErr` (`.log_err()` for failures that matter, `.log_warn()` for best-effort work). Use `.ok()` only where the error is the expected answer (a value that may be absent); when it can also mean a real failure, call `.log_err()` or `.log_warn()` instead.
- Removing something that may not exist: `.missing_is_removed()` (also in `log_err`) before logging, so a normal "not found" does not show up as a failure.
- A failure the user caused or can fix (a save, a key, a file) reaches them as a toast or an inline message, not only the log.
- Logs go through the `log` crate (`log::error!` etc.), never `println!` or `eprintln!` in the app. `DESAFINADO_LOG=debug` shows them.
- `unwrap()` and `expect()` are for tests and for invariants that cannot fail (a constant regex in a `LazyLock`). Compile regexes once in a `static LazyLock`, never inside a function that runs per line.
- Paths are `PathBuf` and `&Path` in new code. File names need not be UTF-8. Older code still carries some paths as `String` (the file watcher, the History key, the last-file record); do not add more.
- Secrets are wrapped: the OpenRouter key is `ApiKey`, whose `Debug` prints `ApiKey(***)`, so a type holding an `ApiKey` may derive `Debug`. Do not derive `Debug` on a type that holds the raw key text, and never log or print a secret.
- `unsafe` is forbidden. Use `rustix` for system calls.
- No `std::env::set_var` in tests: tests run in parallel threads. Pass values in instead.
- Each test asserts something about behavior. No test that only restates a constant or calls code without checking the result.
- Prefer enums to pairs of booleans or `Option<Option<T>>` for state with named cases.

## GPUI

- Only `crates/app` knows about GPUI. Logic goes in the plain crates, with unit tests.
- Disk, network and process work runs in `cx.background_spawn`; the result comes back through `cx.spawn_in` and a `WeakEntity::update`. A synchronous call on the UI thread freezes the window. Known exceptions still to move: the History store writes and the unsaved-text load and flush, reading the file on open and reload, reading Skill files for the unsaved-changes check, path canonicalization during render, the model choice write, and the last-file record (kept synchronous so its save and clear stay in order). Two places run blocking work on a plain `std::thread::spawn` (pricing an AI call, Evals runs); new code uses `background_spawn`.
- `WeakEntity::update` returns a `Result` because the entity may be gone. Handle it or `.log_err()` it.
- Inside `entity.update(cx, |this, cx| ...)` use the inner `cx`, not the outer one.
- Keep the returned `Task` or call `.detach()`. A dropped `Task` cancels the work.
- `gpui-kit` pins the GPUI version (`gpui-pre`). Read its source in `~/.cargo/registry` before guessing an API.
- `gpui-base` is not the registry copy: `vendor/gpui-base` is patched with inlays and block zones ([ADR 0003](docs/adr/0003-inline-suggestions-on-patched-gpui-base.md)). Read the input module there, and on a gpui-kit bump re-apply the patch as [`vendor/gpui-base/PATCH.md`](vendor/gpui-base/PATCH.md) says.

## UI

- All styling goes through `crates/app/src/ui.rs` ([docs/ui-rubric.md](docs/ui-rubric.md)); `crates/app/tests/ui_rules.rs` fails CI otherwise. No borders, type sizes or hover styles outside it, 4px grid, functions of 100 lines or fewer.
- Hierarchy comes from color, not from size or borders. Every theme defines error and success colors.
- Names in code and UI follow the Electron repo's GLOSSARY.md (Suggestion, Block, Quick action, Review, Version).

## Git

- Commit messages: short, imperative, conventional prefix (`feat:`, `fix:`, `refactor:`, ...).
- Pull requests pass the `checks`, `deny` and `gpui-base` jobs before merging.

## Reference

- [Pragmatic Rust Guidelines, agent version](https://microsoft.github.io/rust-guidelines/agents/all.txt) (Microsoft): read the section you need, not the whole file.
- [Zed's AGENTS.md](https://github.com/zed-industries/zed/blob/main/AGENTS.md): GPUI rules from the team that maintains GPUI. Its rules about Zed's own repository and README do not apply here.
