# 0004. Spell checking with Harper and Spellbook

- Status: Accepted (2026-11-24)
- Source: maintainer request for "a kind of spell checker", taken from existing work rather than built from scratch. He chose the engines, then the look (quiet dotted underline, menu under the word) from a clickable prototype.

## Context

The editor is a GPUI text field, not a browser or system text field, so there is no built-in spell checking. The checker has to be a library or a local service the app calls. It must keep up with typing on long texts, work offline, add no cost per call, pass `cargo deny`, and handle English and Brazilian Portuguese, the two languages the maintainer writes in.

| Option | Languages | Grammar | Why not, or why |
|---|---|---|---|
| Harper (`harper-core`, Apache-2.0) | English only | Yes | Chosen for English. Pure Rust, Markdown-aware, a 4,000-word document in about 190 ms. |
| Spellbook (MPL-2.0) + LibreOffice VERO pt-BR | Any Hunspell dictionary | No | Chosen for Portuguese. Pure Rust, used by Helix and Zed's codebook. |
| Hunspell through `hunspell-rs` | Any | No | Same result as Spellbook with a C library; last release 2022. |
| LanguageTool | 30+, the best pt-BR grammar | Yes | A Java server with hundreds of MB of RAM, the weight this app left Electron to shed. Its public API sends the text away. |
| nlprule | en, de, es | Yes | Unmaintained since 2021, no Portuguese. |
| zspell | Any Hunspell | No | Its license fails `cargo deny`. |
| An LLM through OpenRouter | All | Yes | Seconds and money per keystroke. The Review and the Fix grammar Quick action stay the deep, on-demand pass. |

## Decision

1. **Two engines in `desafinado-features::spelling`.** Each Block is checked in the language most of its words belong to (counted against both dictionaries): Harper for English spelling and grammar, Spellbook for Portuguese spelling. Code, HTML and rules are not checked; neither are code spans, links, addresses, acronyms in capitals or words touching digits.
2. **Only what changed is checked.** Issues are kept per Block text; 400 ms after typing stops, the Blocks whose text is new are checked in `background_spawn` and the rest come from that store. Marks an edit touches disappear until then; marks after it move with the text.
3. **Mistakes, not house style.** Harper's Readability, Style and Enhancement lints and its title-case rule for headings are off. Spelling replacements put common words first and, among those, the word with one letter fewer ("comming" → "coming").
4. **The look**: two-pixel dots in the theme's `faint` color, drawn over the editor like the caret, turning `danger` (spelling) or `accent` (grammar) under the pointer or while the menu is open. Clicking a marked word, `Ctrl+.` at the caret, or F8 / Shift+F8 (also in the Edit menu) opens the menu under the word: replacements (the first in bold, `↑` `↓` `Enter`), Add to dictionary (spelling only) and Ignore (`Esc` closes). Text under an AI Suggestion is not marked, and the text map shows no marks.
5. **The writer's words** live in `dictionary.txt` in the settings folder, one per line, for every window. Ignore lasts while the editor is open: a misspelled word everywhere, a grammar issue only for the same words and rule.
6. **The pt-BR dictionary ships in the binary**, gzipped (1.4 MB), under the MPL 2.0 side of its LGPL/MPL dual license. `crates/features/dictionaries/README.md` says where it came from and how to update it.

## Consequences

- Grammar in Portuguese is not checked. The AI Review covers it on demand; an optional local LanguageTool could come later.
- `harper-core` 3.2 pins `zip` 8, which has extraction advisories fixed only in `zip` 9. The app never opens an archive and Harper's Weirpack code is not called, so `deny.toml` ignores those advisories with that reason (approved by the maintainer). Drop them once Harper moves to `zip` 9.
- Harper brings its own dependency tree (a small neural tagger among it); the release binary grows accordingly, and the first check after start waits about a second while both dictionaries load in the background.
- A word that wraps between rows has its mark drawn per word; a mark is not drawn for a word whose two ends fall on different rows.
