# Feature parity with the Electron build

Every feature of `desafinado`'s Electron build (as of `b269c9c`, 2026-11-14) and its state in the native build. Done means it works the same from the writer's side; motion is matched where GPUI allows and noted where not.

## Document and files
- [ ] New (`Ctrl+N`), Open… (`Ctrl+O`), Save (`Ctrl+S`), Save as… (`Ctrl+Shift+S`), asking Save / Discard / Cancel over unsaved text
- [x] Atomic save keeping mode and symlinks; conflict when the disk changed since the last read (External change notice: Reload from disk / Keep mine)
- [x] Unsaved text kept per file across restarts and a killed process; the last file reopens at start (the last file is kept in `last-file.txt` in the user data dir instead of `localStorage`)
- [x] First Markdown argument on the command line opens; one instance per user data dir hands later files to the running app (an `flock` on `instance.lock` and a Unix socket `instance.sock` in the user data dir stand in for `requestSingleInstanceLock`)
- [x] Watcher: an external edit of the clean open file reloads and records an External Version; a dirty one shows the notice
- [ ] Frontmatter: not a Block; frontmatter problems in the File menu; frontmatter panel to edit `name` and `description`

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
- [ ] Library rail (`Ctrl+B`), Library tree of roots, Skills, folders and files; keyboard tree navigation; descriptions; problems in the danger color
- [ ] Library search (`Ctrl+P`)
- [ ] Add a root, Refresh, New skill from template
- [ ] Improve description (AI)

## Evals
- [ ] Evals page "How this Skill performs": last run, Did the Skill fire? / Did it help?, Each Eval table, suspicious cases
- [ ] Eval run through the Claude CLI with Arms, runs 1–5, Cancel, toast at the end, first-eval guide

## Preferences and themes
- [ ] Preferences (`Ctrl+,`): themes (24 Palettes + Desafinado; Omarchy theme when present), Model, OpenRouter key, Quick actions (toggle, order, add, edit, delete)
- [ ] Version and commit at the bottom

## AI and spend
- [ ] OpenRouter called directly, through `HTTPS_PROXY` and `SSL_CERT_FILE` (oneCLI)
- [ ] Key screen and libsecret key store
- [ ] AI call records (task, models, provider, tokens, cost, latencies, finish reason, generation id) in `ai-calls.jsonl`
- [ ] Spend view: calls and total cost

## Shell
- [ ] Custom title bar with File and Edit menus and keyboard menu navigation
- [ ] Toasts, missing key banner, start error screen
- [ ] Installs and runs locally (`cargo install --path crates/app` plus a desktop entry)
