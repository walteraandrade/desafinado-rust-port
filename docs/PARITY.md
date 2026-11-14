# Feature parity with the Electron build

Every feature of `desafinado`'s Electron build (as of `b269c9c`, 2026-11-14) and its state in the native build. Done means it works the same from the writer's side; motion is matched where GPUI allows and noted where not.

## Document and files
- [ ] New (`Ctrl+N`), Open… (`Ctrl+O`), Save (`Ctrl+S`), Save as… (`Ctrl+Shift+S`), asking Save / Discard / Cancel over unsaved text
- [x] Atomic save keeping mode and symlinks; conflict when the disk changed since the last read (External change notice: Reload from disk / Keep mine)
- [x] Unsaved text kept per file across restarts and a killed process; the last file reopens at start (the last file is kept in `last-file.txt` in the user data dir instead of `localStorage`)
- [x] First Markdown argument on the command line opens; one instance per user data dir hands later files to the running app (an `flock` on `instance.lock` and a Unix socket `instance.sock` in the user data dir stand in for `requestSingleInstanceLock`)
- [x] Watcher: an external edit of the clean open file reloads and records an External Version; a dirty one shows the notice
- [x] Frontmatter: not a Block; frontmatter problems in the File menu; frontmatter panel to edit `name` and `description` (the panel sits above the editor instead of scrolling with the text; `Ctrl+Z` in a field undoes the field and writes that back as a new edit instead of undoing the Document; no screen-reader live region)

## Editor and Suggestions
- [ ] Raw Markdown with light styling, Inter, 48rem column, headings bold (bigger headings: see ADR 0001)
- [ ] Bubble (`Ctrl+K`): Quick actions by typing, `↑` `↓` `Enter`, "Ask AI: …" free text
- [ ] Target: selection or the Block under the cursor; Neighbors and title/section as context
- [ ] Suggestion overlay: deleted text struck through, Replacement in the AI color, Changes counted, Rationale
- [ ] Card with Accept (`Ctrl+Enter`), Reject (`Esc`), Refine; Refine steps `‹ n / m ›` (`Ctrl+H` / `Ctrl+L`); `Alt+[` `Alt+]` between Suggestions; `Tab` into the Card
- [ ] Accept is an undoable edit; Discard when the writer edits inside a Suggestion or a newer one overlaps
- [ ] AI caret with flag and Instruction while the AI works
- [ ] Review pass with Focus (Edit → Review…), streamed one Suggestion at a time, Stop; Accept all / Reject all
- [ ] Errors and retries shown as in the Electron build

## History
- [x] Versions with Cause (Started, Edited, Accepted, Opened, Restored, External, Saved mark) in `history.sqlite`, one History per file
- [x] Timeline under the header, a dot per Version, compare with the current text, Restore (`←` `→` `Home` `End` step between dots while a Version or the full history is shown, not from a focused dot in the editing view; Now has no hover title)
- [x] Full history page "How this Document changed" (entry diffs are computed for every entry, not only near the viewport; the flash on a dot click ends without a fade)
- [x] Undo/redo keys (`Ctrl+Z`, `Ctrl+Shift+Z`, `Ctrl+Y`) (opening a file or Reload from disk starts a fresh undo stack, where CodeMirror kept it as one undoable change; Restore and a watcher reload are undoable as in Electron)

## Library and skills
- [x] Library rail (`Ctrl+B`), Library tree of roots, Skills, folders and files; keyboard tree navigation; descriptions; problems in the danger color (the rail buttons are icons with the label and shortcut in their tooltip, since GPUI has no vertical text; the Library and descriptions appear without the slide and unfold motion; rows are one flat list, Tab does not step through the panel)
- [x] Library search (`Ctrl+P`) (no visually hidden result count, GPUI has no screen-reader layer)
- [x] Add a root, Refresh, New skill from template (Add uses the GPUI folder picker, or `DESAFINADO_DEV_PICK_FILE` like the Electron dev switch; New skill picks the folder from a list of rows instead of a dropdown)
- [x] Improve description (AI) (the proposal appears and leaves without the unfold motion)

## Evals
- [x] Evals page "How this Skill performs": last run, Did the Skill fire? / Did it help?, Each Eval table, suspicious cases (the Each Eval table is rows of flex cells; a flag's full wording is its tooltip, with no help cursor; inline `code` in the guide's sentences is plain text, the example blocks use DejaVu Sans Mono; Open report hands the file to `xdg-open`; the Evals icon breathes with GPUI's opacity animation)
- [x] Eval run through the Claude CLI with Arms, runs 1–5, Cancel, toast at the end, first-eval guide (Compare against is a list that unfolds under the field instead of a native select; Runs and Cost limit are text fields with the same checks, no spinner; `DESAFINADO_CLAUDE_CLI=<path>` names the CLI before PATH and `~/.local/bin`, a port addition for checks, with `scripts/fake-claude` as a stand-in that never calls the model; quitting during a run waits up to 6 s for it to stop, as in Electron)

## Preferences and themes
- [x] Preferences (`Ctrl+,`): themes (24 Palettes + Desafinado; Omarchy theme when present), Model, OpenRouter key, Quick actions (toggle, order, add, edit, delete) (the panel does not make the page behind it inert, as the modal `<dialog>` does; a theme applies at once without the view-transition crossfade; the choice is kept in `theme.txt` and the model in `model.txt` in the user data dir instead of `localStorage`; the Omarchy theme is read from `~/.local/state/omarchy/current` at start in every build, not only in dev, and is not followed live; a deleted Quick action leaves without the list motion, and focus returns to the panel rather than to the neighboring control; the AI section has one more row, AI spend)
- [x] Version and commit at the bottom (the native crate version; `build.rs` reads the commit from `.git` like `config/build-info.ts`, `unknown` outside a checkout, `DESAFINADO_COMMIT` overrides it for packaging)

## AI and spend
- [ ] OpenRouter called directly, through `HTTPS_PROXY` and `SSL_CERT_FILE` (oneCLI)
- [x] Key screen and libsecret key store (the OpenRouter key dialog opens from the missing key banner and from Preferences, with the same states, plain-text offer and Remove key; a saved key reaches the next AI call without a restart; checked in Xvfb through the session and plain-text paths, since Xvfb has no Secret Service)
- [ ] AI call records (task, models, provider, tokens, cost, latencies, finish reason, generation id) in `ai-calls.jsonl`
- [x] Spend view: calls and total cost (the Electron build has no in-app view, only `ai-calls.jsonl` and `window.__aiCalls`; the native page, File → AI spend… or Preferences → AI spend, shows total US$, calls and priced calls, average per call, tokens, failed and retried calls, a 14-day chart, cost by model and by task, and the newest 50 calls, filtered by Today, 7 days, 30 days or All time)

## Shell
- [x] Custom title bar with File and Edit menus and keyboard menu navigation (`↑` `↓` skip disabled items and wrap, `←` `→` switch menus, `Enter` or `Space` choose, `Esc` closes, `Tab` leaves; the menu buttons are not Tab stops in GPUI, so `Alt+F`, `Alt+E` and `F10` open a menu with its first item active, a port addition; `Esc` returns focus to where it was instead of to the menu button)
- [x] Toasts, missing key banner, start error screen (toasts take any number of actions, each a button that dismisses the toast; the start error screen shows when the user data folder cannot be created or written, the native counterpart of a failed platform kit)
- [ ] Installs and runs locally (`cargo install --path crates/app` plus a desktop entry)
