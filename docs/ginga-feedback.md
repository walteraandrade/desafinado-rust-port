# ginga feedback

A friction log kept while moving Desafinado's motion onto [ginga](https://github.com/walteraandrade/ginga) (rev `44f572c`). Each entry is written when the friction happens: what the code wanted to say, what it had to say instead, and where. An evaluator who did not write the code then sorts each one as a gap in ginga, a misuse of ginga, or a need only Desafinado has; its verdicts are under each batch.

Nothing here is filed on the ginga repository until Waltinho approves it.

## Batch 1: the ways out built by hand

Library close, Preferences close, removed Quick action rows. Each kept a `Task` that slept `MOTION_LEAVE` and then dropped a ghost, next to a GPUI `with_animation` keyed by an id that changed between "in" and "out".

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

### 7. Exit-only presences take two calls

- Same as the Ask bubble in #53: a ghost that only leaves is `Presence::present(ghost)` then `.hide(exit, cx)`. Not hit in this batch, because the Library and Preferences now arrive through their presence too, but noted since it is the pattern the codebase already has.

### What got simpler

- `struct Leaving` (two `Task`s and a `library_reopened` flag) is gone. Reopening the Library or Preferences on its way out now turns around from where it is, where before it snapped open (the flag existed to stop GPUI's `with_animation` restarting from zero).
- `crates/ai/src/ui/list_motion.rs` (the diff of two id lists into enter/move/remove steps, 153 lines with tests) is gone: `PresenceList::sync` + `place_all` do it.
- Reduced motion for the panels comes from `Presence` itself instead of an early `return` in each close function.
