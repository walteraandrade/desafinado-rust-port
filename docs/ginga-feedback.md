# ginga feedback

A friction log kept while moving Desafinado's motion onto [ginga](https://github.com/walteraandrade/ginga) (rev `44f572c`). Each entry is written when the friction happens: what the code wanted to say, what it had to say instead, and where. An evaluator who did not write the code then sorts each one as a gap in ginga, a misuse of ginga, or a need only Desafinado has; its verdicts are under each batch.

Nothing here is filed on the ginga repository until Waltinho approves it.

## Batch 1: the ways out built by hand

Library close, Preferences close, removed Quick action rows.

The Quick action rows landed on main first, through #54 (`desafinado_ai::list_rows::ListRows`), while this batch was in review; this batch then took #54's version and dropped its own `quick_action_rows.rs`. The items about the list (1, 5, 6) describe the version written here, and still stand as feedback on ginga: #54 met the same places (its `ListRows` places rows when the list changes, the fix for item 6's post-sweep case). Each kept a `Task` that slept `MOTION_LEAVE` and then dropped a ghost, next to a GPUI `with_animation` keyed by an id that changed between "in" and "out".

### 1. No GPUI adapter for `PresenceList`

- Wanted: `self.rows.frame(window, cx)`, as `ginga_gpui::Presence::frame` does: it reads the clock and asks for the next frame while something moves.
- Had to: keep a `ui::MotionClock` next to the list, call `clock.now(cx)` before every `sync`, `place_all` and `frame`, OR `row.moving()` over the items by hand and call `window.request_animation_frame()` (`crates/app/src/preferences_view.rs`, `row_frames` and `plan_motion`).
- Also: `unless_reduced` has to be applied to each half of the `InOut` and to the glide by the caller (`ui::list_in_out`, `ui::list_glide`), where `ginga_gpui::Presence::show` does it for you. Forgetting it is silent: the list moves under reduced motion.

### 2. `ginga_gpui::Presence` cannot say what it holds without a frame

- Wanted: one field for Preferences, `Presence<Entity<PreferencesPanel>>`, asked "which panel is wanted?" from event handlers (`self.preferences_way.wanted()`).
- Had to: keep `preferences: Option<Entity<PreferencesPanel>>` as the open state and `preferences_way` beside it, and keep the two in step by hand in `open_preferences` and `close_preferences` (`crates/app/src/workspace.rs`). `is_wanted()` answers yes or no but not with the content, and `frame` needs a `Window`, which handlers that only hold `cx` do not have. The engine has `sample(now)`; the adapter does not expose it.
- The Library has no content, so `is_wanted()` was enough there and `library_open: bool` could go: `fn library_open(&self) -> bool { self.library_way.is_wanted() }`.

### 3. The declarative `present` cannot draw with its progress

- Wanted: `ginga_gpui::present("library", open.then_some(()), ways, ..)`, which keeps the leaving content, covers it from clicks and needs no field.
- Had to: use `Presence` and draw by hand, because the Library column's width follows the progress (`ui::column_at`), and `present` only moves an offset and an opacity (`Present::arrive_from` says the look's size is left out). Its `draw` closure gets `&T`, not the progress or the phase.
- Also: with `present`, nothing tells the view when content has finished leaving, which the Library column needs to stop being laid out.

### 4. The input cover while leaving is the caller's job with `Presence`

- `present` takes no clicks while it leaves. `Presence` hands back `Phase::Exiting` and the caller covers the content: a `Way { progress, leaving }` struct in `workspace.rs` exists only to carry `shown.phase == Phase::Exiting` past the borrow of `frame`.
- Not wrong, but every user of `Presence` writes the same check (`ui::toast_takes_clicks` is a third copy).

### 5. `Shown` borrows the presence, so drawing has to copy out first

- `frame(&mut self, ..)` returns `Shown<'_, T>`, which borrows the field mutably. Drawing a row needs `&self` methods (`render_quick_action`) and `&mut Context`, so the frame is copied into an owned `RowFrame` per row (`preferences_view.rs`), keys and labels cloned, before any drawing.
- For `Presence<Entity<..>>` the same means `shown.content.clone()` in `with_overlays`.
- A `frame` that returned owned, `Copy` facts (phase, progress, offset) and left the content to `sample`/`get` by key would avoid the copy.

### 6. A row that finishes leaving shifts the rows below, and `place_all` sees that as a move

- The removed Quick action row collapses its height with its progress (`ui::collapsed`), so the rows below follow the layout up. Its place has to stay counted as a full row while it leaves, or `place_all` would make the rows below glide on top of the layout's move.
- When the row is swept out, every row below gets a place one row higher. `place_all` with the glide tween would draw them one row too low and glide them back. So `row_frames` calls `place_all(.., Transition::Instant, now)` on every frame and only `plan_motion` (right after a `sync`) passes the glide.
- Side effect: a row still gliding from a reorder when another row's exit ends is snapped to its place. Rare (a glide is 260 ms, an exit 180 ms) but real.
- The docs suggest a running total that follows the leaving row's height, but with a tween that "starts over at each change" the rows below lag behind the layout; only a spring follows it smoothly, and the old motion was a 260 ms OutExpo tween.
- Wanted: a way to tell the list that a place change comes from layout (no travel), or a `place_all` that handles "an item before this one was swept" itself.
- Found in review: `sync` sweeps too, so a change made right after a row finished leaving (before a frame ran) glided the rows below one row wrong. `QuickActionRows::change` now re-places instantly before it syncs (`crates/ai/src/quick_action_rows.rs`, tested).

### 7. Exit-only presences take two calls

- Same as the Ask bubble in #53: a ghost that only leaves is `Presence::present(ghost)` then `.hide(exit, cx)`. Not hit in this batch, because the Library and Preferences now arrive through their presence too, but noted since it is the pattern the codebase already has.

### What got simpler

- `struct Leaving` (two `Task`s and a `library_reopened` flag) is gone. Reopening the Library or Preferences on its way out now turns around from where it is, where before it snapped open (the flag existed to stop GPUI's `with_animation` restarting from zero).
- `crates/ai/src/ui/list_motion.rs` (the diff of two id lists into enter/move/remove steps, 153 lines with tests) is replaced by `crates/ai/src/quick_action_rows.rs`, a thin wrapper over `PresenceList::sync` + `place_all` with the same cases as tests.
- Reduced motion for the panels comes from `Presence` itself instead of an early `return` in each close function. Opening them under reduced motion is now instant too; before, only closing was.

### Evaluator's verdicts

An independent evaluator (fresh context, given the diff and this log, not the implementer's conclusions) checked each claim against ginga's source. A = gap in ginga, B = misuse or a feature that was missed, C = Desafinado's own need.

| Item | Verdict | Note |
|---|---|---|
| 1. No adapter for `PresenceList` | A | The README's own GPUI list example has the same boilerplate. |
| 2. `Presence` has no content read | B | Preferences only slides and fades, which `ginga_gpui::present` already does (and it would also blur focus while leaving). A small `wanted() -> Option<&T>` is still missing. |
| 3. `present` cannot draw with progress | A | Only for the Library column's width. |
| 4. Input cover is the caller's | A, low | `present` covers and blurs; `Presence` users write it each time. |
| 5. `Shown` borrows | B for the list, A (small) for `Presence` | `PresenceList::sample(&self)` exists; `&mut cx` does not clash with `&self`. |
| 6. A sweep looks like a move | A | Plus the unlogged post-sweep `sync` case, now fixed here. |
| 7. Exit-only takes two calls | A, trivial | |

Friction the log missed:

- U1 (A): `slid`, `slide_at`, `column_at`, `offset_by`, `collapsed` (and `lifted` from #53) all re-implement "apply a `Look` mixed by a progress", because `ginga_gpui`'s `styled` is `pub(crate)`. The only public way to apply a look is `.look(id, look, Instant)`, which keeps a motion in element state for nothing.
- U2 (A, part of 1): `list_in_out` is `panel_in_out` with `unless_reduced` around each half; `InOut` has no reduced form.
- U4 (B, design): the list's content could be the whole slot instead of a label, drawing every row from `row.content`.
- U7 (A, part of 4): a leaving panel keeps keyboard focus (as before); `present` would blur it.

Fixed in this batch after review: rows arriving or leaving now keep their glide offset; `close_preferences` notifies, so Preferences slides out even when Spend was already open.

Not changed: Preferences stays on `Presence` instead of `present` for now. `present` would make a reopen during the exit a swap of two panels, where `Presence` turns the one panel around. Worth trying in batch 2, where the entries move too.

### Proposed ginga issues (waiting for Waltinho's approval)

1. `ginga_gpui::PresenceList` with its own clock, reduced motion and frame requests (about 30 lines here).
2. Place changes caused by a sweep are not travel: keep a glide in progress (fixes a visible snap).
3. Apply a look to an element without motion: make `styled` public, e.g. `element.with_look(look)` (about 34 lines here).
4. `present` that hands its progress and phase to `draw`, or honors size in `arrive_from`/`leave_to` (the Library column).
5. Cover and blur for `Presence` while it leaves, e.g. `shown.cover(element)`.
6. Small reads on `Presence`: `wanted() -> Option<&T>`, `sample(cx)`, and a `leaving(content, exit, cx)` constructor.

## Batch 2: the ways in

Every `ui::enter`, `ui::enter_after` and `ui::enter_in_turn` (the Ask bubble and its rows, Suggestion cards, the note card, the Spend page, the AI caret's flag) now arrive through a declared look: `element.look(id, rest, tween.delay(d)).from(start)` in `ui::entering`. The File and Edit menus and the Review popover already came and went through `Presence` (#56); the Library and Preferences through #57.

### 8. A look's offset replaces an absolutely placed element's place

- Wanted: `card.look(id, Look::new().offset(0., 0.).opacity(1.), way).from(Look::new().offset(8., 0.).opacity(0.))` on a Suggestion card that is `.absolute().left(x).top(y)`, so it slides 8 px into its place.
- Got (documented on `Looked`): for an absolute element the offset *is* `left`/`top`, so the card would jump to its parent's corner. The docs say to give the element its place as the offset, but then any later change of place (a scroll, a reflow) is animated by the same tween: cards would lag behind the text.
- Had to: in `ui::entering`, an absolute element enters inside a layer `div().absolute().top_0().left_0().size_full()` that covers its parent, and the layer takes the look (`crates/app/src/ui.rs`). The AI caret's flag, placed with `right(0)` when flipped, needed this too: a `left` from the look would have stretched it.
- Wanted: an offset that adds to the element's own `left`/`top` (a translate), at least when they are in pixels; or a `Looked::translate` that never touches the element's place.

### 9. `.look()` turns the element into `Looked<E>`

- The old helpers returned `AnimationElement<E>`; these return `AnyElement`. No caller needed `E` back, so it cost nothing here. `Looked<E>` still takes children (`ParentElement`), but not more styling or `on_click`, so the look has to come after those. (The `AnyElement` itself comes from item 8's two branches, not from `Looked`.)

### 10. What got simpler

- A delay is `Tween::delay`, which holds the start look until it is over: the `lead_of`/`delayed` easing wrapper is no longer needed for entries (it was still used by the Spend bars until batch 3).
- Reduced motion: `Looked` takes a look at once when the desktop asks for less motion, as GPUI's `with_animation` already did. No change there.
- But under reduced motion ginga's `unless_reduced` turns a delayed tween into `Transition::Instant`, which drops the delay too. The Suggestion card shown on hover used that 160 ms delay as its hover intent. That wait is now a timer in `EditorPane` (`hover_card`, `wait_for_hover_card`), so it no longer depends on motion at all; the card simply arrives 160 ms after the pointer rests on the text.
- `Rise` and `Slide` moved the element with a margin, which also pushed what came after it in the layout; a look's offset is `relative`, so only the element moves. No visible difference on the surfaces that use them (checked in the recordings).

### 11. No presets for common ways in

- Desafinado keeps its own `Enter::{Rise, Slide, Lift}` and `ui::enter_looks` to turn one into a pair of looks. ginga's examples build the same `Look::new().offset(..).opacity(0.)` by hand each time. A Desafinado need more than a gap (C), unless ginga wants presets like `Look::rise(px)`.

### Evaluator's verdicts (batch 2)

| Item | Verdict | Note |
|---|---|---|
| 8. Offset replaces an absolute element's place | A | `styled()` in `look.rs` sets `left`/`top`; a relative element's own `left`/`top` are overwritten too. GPUI has a paint-only offset (`with_element_offset`) a `translate` could use; it would delete about 15 lines here. |
| 9. `Looked<E>` | A, minor | Corrected above: children still work. |
| 10. Simpler | – | The reduced-motion claim was wrong and is corrected above; the delay claim holds (`transition.rs`, `motion.rs`). |
| 11. No presets | C | |

Unlogged friction it found:

- The cover layer becomes the `Looked`'s bounds, so a hover or pressed look on it would react anywhere over the pane. Not used today. (A, part of 8)
- Branching on `style().position` is fragile: an element made absolute after `ui::enter` would be missed. Now in one tested function, `enters_in_a_layer`. (A, part of 8)
- `Enter::Lift` and `Enter::Rise` now behave the same (both a relative offset). (C)

Reviewer: no bugs in placement, hit testing (the layer has no hitbox), paint order, ids or timing; it asked for the layer choice to be tested (done) and found the reduced-motion delay (fixed with the timer).

### Proposed ginga issues from batch 2

7. A paint-only `translate` for looks, so an offset never replaces an element's own place (fixes item 8 for absolute and relative elements).
8. Reduced motion keeps a transition's delay: `unless_reduced` should turn a delayed tween into an instant move *after* the delay, or document that it drops it.

## Batch 3: what moves for as long as it is shown

The breath of the AI caret's bar and of a working request's mark in the tray, the working dots (the AI caret's flag and the Refining button), and the Evals icon's breath now come from `ginga_gpui::Keyframes`; the Spend page's bars grow through a declared look (`Look::width` / `Look::height` with a delayed `Tween`). Desafinado's own `ease_out_expo`, `ease_in_out_sine`, `lead_of` and `delayed` are gone: `Easing::OutExpo` / `Easing::InOutSine` and `Tween::delay` replace them. No `with_animation` is left in the app. The cursor blink is a toggle, not an animation, and stays as it is.

### 12. Keyframes are a view's state; there is no keyframes element

- Wanted: what `look` is for a one-off change, for a loop: `mark.keyframes(id, breath, |mark, opacity| mark.opacity(opacity))`, kept under an id, so `ui::job_mark`, `ui::ai_dots`, `ui::gauge_icon` and the caret's bar stay plain functions of their state, as they were with `with_animation`.
- Got: `ginga_gpui::Keyframes` must be owned by a view and called with `frame(window, cx)` while it renders.
- Had to: give each owner a field (`AiCaret`: `breath`, `dots`; `TrayMarks`: `breath`; `EditorPane`: `refining`; `EvalsView`: `gauge_breath`) and turn the helpers into functions of a value (`JobMark::Working { opacity }`, `dot_levels`, `gauge_icon(opacity)`, `ai_caret_bar(opacity)`). `EditorPane` also keeps the dots' levels for the frame in `refine_dots`, because the card is drawn from `&self` methods. The Evals icon is drawn by `Workspace`, whose `new` is at the 100-line limit, so its breath lives in `EvalsView` and `Workspace` asks for it (`EvalsView::gauge`).

### 13. A loop that leaves the screen does not start over when it comes back

- `with_animation` keeps its start in the element's state, so a mark that is drawn again starts from the top of its breath. `Keyframes` start on the first frame that draws them and then run on: a caret that shows again comes back mid-breath, at whatever opacity the time gives.
- Had to: `ui::Looping`, a wrapper with `frame` (restarts the keyframes on the first frame after an absence) and `away` (called on each frame that leaves the loop out). Every owner has to remember to call `away` on each path that does not draw, and when the owner is not rendered at all (the editor hidden behind Spend, Evals, Reading or the history), someone else has to call it: `Workspace` does, through `ai_caret_view::land` and `EditorPane::loops_away`.
- Wanted: item 12 would fix this for free (state kept under an id dies when the element is not drawn), or a `Keyframes::restart_when_drawn()`.

### 14. Staggered items cannot share one loop at different points of it

- The three working dots run on one 1.2 s round, each 160 ms after the one before, and each dot's fall wraps over the end of the round into the next. The gallery's pulsing dots each wait inside the round, which only works when the rise and fall leave room; ours fill the whole round.
- The engine's `Keyframes::starting_at` would at least start each dot later, but `ginga_gpui::Keyframes` does not mirror it (only `restart`), and before its start it rests on the first stop instead of being partway through a round.
- Had to: one linear `0 → 1` round (`ui::dots_round`), and the dot curve stays in Desafinado (`dot_levels`, `dot_opacity`, now on `Easing::InOutSine`).
- Wanted: a phase for keyframes (`.at_phase(0.13)` or `.offset(d)` that starts partway through a round), on both the engine and the adapter.

### 15. ginga's `InOut` is not GPUI's `ease_in_out`

- The Evals icon breathed with GPUI's `bounce(ease_in_out)`, a quadratic in-out. ginga's `Easing::InOut` is cubic. To keep the feel: `Easing::Custom(ease_in_out)` with GPUI's function, which works because it is a plain `fn`. A mirrored round of two stops is the same as `bounce`, so that part maps cleanly.

### 16. `at_most_every` caps one set of keyframes, not a view

- The tray's marks used to breathe each on its own, from when each appeared. One `Keyframes` per mark would give each its own frame timer, so the tray would ask for frames up to once per mark per 33 ms. They now share one breath and breathe together. A visible change, small: two marks that run at once are in step.
- In `Workspace` and `EditorPane`, which are not islands, other motions ask for every frame anyway while they move, as the docs say.
- The Refining dots of every Suggestion share one round too: a refine started later joins it mid-round, where each button used to start its dots from the top. They run only while a Refining card is drawn (`refining_on_screen`).
- The Evals icon's breath had no frame cap; it now has the same 33 ms one as the other loops (about 20 frames a second at 60 Hz, as GPUI's `with_max_fps(30.)` gave).

### 17. What got simpler

- The ease curves and the delay arithmetic are ginga's now. The Spend bars are a look with a delayed tween, as the entries are. A change of period still grows the share bars from nothing (their ids include the period). When the data under the same ids changes (Reload, or the 14 days rolling over at midnight) the bars now glide to their new length after their cascade delay (up to 336 ms) instead of jumping; a row can glide from the size of the model or day that held its place before.
- Reduced motion: GPUI's `with_animation` already rested a loop on its start and a one-shot on its end; `Keyframes` rest on their first stop and `Looked` jumps to its look. Same result on screen.

### Evaluator's verdicts (batch 3)

| Item | Verdict | Note |
|---|---|---|
| 12. No keyframes element | A | Nothing keeps keyframes under an id; `look`/`look_with` only move toward a target. `Island` keeps state as a view's field too. GPUI's `window.use_keyed_state` could hold them but needs `&mut Window`, which the card methods do not have. |
| 13. Loop does not start over | A | True: the clock starts at the first frame and runs on. The first version missed owners that are not rendered at all; fixed with `land` / `loops_away`. |
| 14. Staggered dots | A | All claims hold; also under reduced motion the linear round rests on 0, so `dot_levels` needs its own `still`. |
| 15. `InOut` vs `ease_in_out` | C | `Custom` takes GPUI's curve exactly; ginga could document that `InOut` is cubic. |
| 16. `at_most_every` per keyframes | A, low | Overstated at first: GPUI's `with_max_fps` also kept one timer per element, so breathing in step is a saving, not a cost ginga forced. |
| 17. Simpler | – | Reduced-motion claims hold; the Spend glide claim was wrong and is corrected above. |

Unlogged friction it found:

- The Evals icon and the Refining dots are capped loops outside an island, against ginga's advice: during an Evals run the whole `Workspace` renders at about 20 frames a second (it rendered at 60 before). An island for the icon would render only it. (B, left for later)
- `dots_round` is keyframes used as a clock, with no rest of its own (part of 14).
- Every literal set of keyframes needs `.expect(..)`; a round-trip preset would cover all three loops here. (A, minor)
- The Refining dots' shared round and the gauge's new frame cap were not logged; now they are (16).

Reviewer: one bug, the Refining dots asked for pane frames while no Refining card was drawn (fixed: `refining_on_screen`, tested); loops not restarting when the editor came back from another screen (fixed); a wrong comment on the frame cap (fixed); the loops' periods and depths untested (now tested through the engine's `Keyframes::sample`). It found the timing, depths, easing, stagger, cascade, reduced motion and the view that frames are asked for all as before.

### Proposed ginga issues from batch 3

9. A keyframes element kept under an id, dropped when it is not drawn (fixes items 12 and 13, and gives each item its own loop).
10. A phase for keyframes that starts partway through a round, on the engine and the adapter, and `starting_at` in the adapter (item 14).
11. Low: one frame-cap timer per view instead of per keyframes, or a doc line that GPUI's `with_max_fps` behaves the same (item 16).

