# UI rubric

Ten criteria for the UI code and what it draws. Each one is measured from
the source or from the themes, not by taste. Seven are tests that run in CI:
`crates/app/tests/ui_rules.rs` (C1, C2, C3, C6, C8, C9, C10) and
`every_used_pair_meets_aa_in_every_theme` in `crates/app/src/theme.rs` (C5).

Scoring: a counted criterion scores `10 − ⌈violations / 5⌉`, at least 0. A
checklist criterion scores `⌊10 × passed / total⌋`.

| # | Criterion | How it is measured | Start | Now |
|---|---|---|---|---|
| C1 | Colors come only from theme roles | No `hex(`, `rgb(`, `rgba(`, `hsla(`, `black()`, `white()` or `transparent_*` outside `theme.rs`; no `opacity(` outside `ui.rs` | 9 (5) | 10 (0) |
| C2 | Hierarchy by color, not borders | No `.border_*` or `roles.hairline` outside `ui.rs` and `theme.rs`. Inside `ui.rs`, borders only for the focus ring, an invalid field and empty data marks | 0 (98) | 10 (0) |
| C3 | Hierarchy by color, not size | No `text_size`, `text_*()`, `font_weight` or `font_family` outside `ui.rs`; the scale is 12 and 14 px for the UI, 17 px for Document text and 24 px for the wordmark; the one exception is the Reading view's rendered headings, which scale from the Document size (`HEADING_SCALE` in `ui.rs`) because they are the Document's own hierarchy | 0 (163 uses, 10 sizes) | 10 (0, 4 sizes) |
| C4 | Semantic color on states | 12 states: AI work in the AI color (pending text, AI pips, AI Versions, ask bubble), failures in danger (toasts, failed cards, failed calls, error text), success in success (saved and created toasts), destructive buttons (Discard, Delete, Remove key) in danger | 7 (9/12) | 10 (12/12) |
| C5 | Contrast meets WCAG AA | Every text color on every surface the components draw it on is at least 4.5:1, every mark and the focus ring at least 3:1, in all 25 themes. `faint` is only for disabled text | 2 (≈40 faint-as-text uses) | 10 (0 pairs below AA) |
| C6 | Every click target is a shared component | Every `on_click` and left `on_mouse_down` listener sits on an element built by `ui::` | 2 (36) | 10 (0) |
| C7 | Overlays stay usable | 12 checks: inside the window at the minimum size (menus, popovers, cards, dialogs, Preferences, Library), close on Esc, close on a click outside, never cover their target. Fixed: a minimum window size of 720 × 540, the Review popover's width, the ask bubble flipping over the selection, Preferences closing on a click outside | 6 (8/12) | 10 (12/12) |
| C8 | Pointer and hover only through shared components | No `.cursor_pointer()`, `.hover(`, `.cursor(CursorStyle` or `bg(roles.fill_hover)` outside `ui.rs` | 2 (36) | 10 (0) |
| C9 | Functions stay short | No function over 100 lines in `crates/app/src` | 6 (20) | 10 (0) |
| C10 | Spacing sits on the 4 px grid | Every literal padding, margin and gap is a multiple of 4 | 0 (48) | 10 (0) |

## The shared components

`crates/app/src/ui.rs` decides every size, weight, border, hover and click
affordance. Screens only arrange its parts: `title`, `meta`, `label`,
`detail`, buttons by kind (`Default`, `Strong`, `Quiet`, `Primary`,
`Danger`), `toggle_button`, `option_row`, `focusable_row`, `check`, `chip`,
`link`, `tag`, `badge`, `pip`, `field`, `select_field`, `toast_frame` by tone,
`dialog_frame`, `popover_frame` and `modal_backdrop`.

## Screenshots

Before on the left, after on the right, dark Mood Indigo unless named.

- Preferences: `docs/screenshots/ui-rubric-1-preferences.png`
- Full history: `docs/screenshots/ui-rubric-2-history.png`
- Ask bubble below the selection: `docs/screenshots/ui-rubric-3-bubble.png`
- Suggestion card: `docs/screenshots/ui-rubric-4-card.png`
- AI spend: `docs/screenshots/ui-rubric-5-spend.png`
- Satin Doll (light): `docs/screenshots/ui-rubric-6-satin-doll.png`
- Preferences at the minimum window size: `docs/screenshots/ui-rubric-7-smallest-window.png`
