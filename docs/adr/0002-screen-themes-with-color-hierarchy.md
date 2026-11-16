# 0002. Screen themes with a color hierarchy

- Status: Accepted (2026-11-15)
- Source: maintainer request. The themes carried over from the Electron app ([desafinado 0017](https://github.com/walteraandrade/desafinado/blob/main/docs/adr/0017-book-themes-and-solid-colors.md)) painted the exact codes of *A Dictionary of Color Combinations*, a book of palettes for clothing, homes and objects. They never worked well on screen: lemon-yellow pages, one ink for every kind of text, faint text that was the accent, no success color. The maintainer wants every theme rebuilt for screen, with defined error and success colors, and wants color, not size or borders, to carry the hierarchy.

## Decision

1. **A theme is a seed, not a list of codes.** `crates/features/src/theme/screen_palettes.rs` keeps one `ThemeSeed` per theme: mode, the hue and chroma of its neutrals (`base`), the hue and chroma of its headings, and the hues of accent, AI, danger and success. The hues come from the book combination the theme was named after, so each theme keeps its name and mood; the colors themselves are computed.
2. **Roles are computed in OKLCH.** `roles_of_seed` lays the surfaces on a fixed lightness ladder per mode (canvas, raised, overlay, fill, fill hover, selection), tinted by the base hue, then finds each text role by walking lightness until it reaches a contrast target on every surface it sits on: heading 7:1, ink 8.5:1, muted 5:1, faint 3.2:1, accent, AI, danger and success 5:1, each signal also on its own fill. Chroma is lowered to stay inside sRGB. Colors are solid, as before.
3. **19 roles.** New: `overlay` (floating surfaces), `heading`, `muted`, `ai_fill`, `danger_fill`, `success`, `success_fill`. `faint` is no longer the accent.
4. **Hierarchy by color.**
   - Headings in the Document, dialog and panel titles, and the wordmark use `heading`, a saturated hue apart from the body ink. Body text is a near-neutral `ink`; labels and secondary text are `muted`; Markdown punctuation and disabled text are `faint`.
   - Regions are told apart by surface color, not borders: the header and title bar are `raised`, menus, popovers, toasts, dialogs and Cards are `overlay`, and the old hairlines around them are gone. Hairlines stay only as row dividers in tables and lists.
   - Buttons have no border: Default is `fill`, Strong (the one non-AI affirmative action) is `accent` with canvas text, Primary stays the AI color.
   - States are fills: an active Suggestion and the highlighted ask row use `ai_fill`, a failed Card `danger_fill`, the Saved toast `success_fill` with `success` text, passing Eval marks `success` and failing ones `danger`.
5. **Tests guard the rules.** `catalog.rs` checks, for all 25 themes: AA for every text and signal pair, surfaces apart by at least 0.02 in OKLab, ink above muted above faint, headings at least 0.04 more chroma than ink, danger red, success green, and the AI color apart from danger, success, ink and accent.
6. The Omarchy theme computes the new roles from its `colors.toml` (`red`/`color1`, `green`/`color2`, `magenta`/`color5`), with fallbacks.

## Consequences

- The Rust port's themes no longer match the Electron app's. The seeds and `roles_of_seed` are plain arithmetic and would port to TypeScript as is, if the Electron app should follow.
- The book's exact codes and the Wada color data are gone from this repo; a theme's origin line still names its book combination.
- Adding a theme is one seed; contrast is the code's job, the tests catch hue clashes.
