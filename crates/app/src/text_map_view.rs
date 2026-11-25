//! The text map, which stands where the scrollbar was: the Document in miniature, with the
//! Suggestions in it and the part on screen; a click or a drag scrolls there. And a chip at the
//! top and the foot of the text that counts the Suggestions past that edge and goes to the
//! nearest one.

use desafinado_core::{
    log_err::LogErr,
    suggestions::{RefineState, Suggestion, active_suggestion, suggestion_anchor},
    text_map::{
        MapLine, MapMark, MarkTone, TextMap, View, fill_rows, line_shapes, locate, map_lines,
        mark_near, off_screen, reveal_first_row, scroll_row_for, step_in_view, text_map,
    },
};
use gpui_kit::{
    base::input::{BufferPoint, EditorScrollbarRenderer},
    *,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use crate::{editor_pane::EditorPane, theme::Roles, ui};

const MAP_WIDTH: f32 = 40.;
/// From the right edge of the text to the map.
const MAP_GAP: f32 = 8.;
const MAP_INSET: f32 = 8.;
const CHIP_INSET: f32 = 8.;
/// How close a click on the map must land to a Suggestion's bar to open it.
const MAP_REACH: f32 = 6.;

/// What the pane keeps for the map between frames.
#[derive(Default)]
pub(crate) struct TextMapState {
    /// Bumped on every edit, so the lines are measured again only after the text changed.
    revision: u64,
    lines: RefCell<Option<(LinesKey, Rc<[MapLine]>)>>,
    /// A drag that started on the map, with the map it started on and its top edge.
    drag: Option<(Rc<MapClick>, Pixels)>,
    /// Where wheel turns over the map have scrolled to before the editor applied them.
    wheel_target: Cell<Option<Pixels>>,
    /// When the text last changed, to draw the map from the lines measured before while the
    /// writer is still typing.
    last_edit: Option<Instant>,
    /// Redraws the map once the typing stopped.
    settle: Option<Task<()>>,
    /// The lines of the text at a revision, measured apart from the editor once the typing
    /// stopped; only their rows are left to read from the editor.
    shapes: Option<(u64, Vec<MapLine>)>,
}

/// How long after the last edit the map keeps the lines it measured before the edit.
const MAP_SETTLE: Duration = Duration::from_millis(250);

impl TextMapState {
    pub(crate) fn edited(&mut self, now: Instant) {
        self.revision = self.revision.wrapping_add(1);
        self.last_edit = Some(now);
    }

    fn typing(&self, now: Instant) -> bool {
        self.last_edit
            .is_some_and(|edit| now.saturating_duration_since(edit) < MAP_SETTLE)
    }
}

/// Whether the lines must match the text now, or the ones measured before the last edits will do.
#[derive(Clone, Copy, PartialEq)]
enum Lines {
    Exact,
    Drawn,
}

/// What the measured lines depend on besides the text: wrapping follows the width.
#[derive(Clone, Copy, PartialEq)]
struct LinesKey {
    revision: u64,
    len: usize,
    wrap_rows: usize,
    width: Pixels,
}

impl LinesKey {
    /// Whether lines measured for `self` can be drawn for `key`. While the writer types, or
    /// while the editor still estimates the rows of a large Document, a frame that only draws
    /// keeps the lines measured before, as long as the text and the width stay the same.
    fn fits(&self, key: &LinesKey, kind: Lines, typing: bool, estimating: bool) -> bool {
        self == key
            || (kind == Lines::Drawn
                && self.width == key.width
                && (typing || (estimating && self.revision == key.revision && self.len == key.len)))
    }
}

/// What the map and the chips read from the editor on one frame.
struct MapFrame {
    lines: Rc<[MapLine]>,
    marks: Rc<[MapMark]>,
    ids: Rc<[String]>,
    view: View,
    line_height: Pixels,
}

impl MapFrame {
    fn rows(&self) -> Vec<usize> {
        self.marks.iter().map(|mark| mark.row).collect()
    }

    /// The Suggestion nearest the top edge of the screen from above, or the bottom edge from below.
    fn nearest_past(&self, below: bool) -> Option<&str> {
        let rows = self.rows();
        let top = self.view.first_row;
        let bottom = top + self.view.rows;
        let mut past = rows.iter().zip(self.ids.iter());
        let found = if below {
            past.find(|(row, _)| (**row as f32) >= bottom)
        } else {
            past.rev().find(|(row, _)| (**row as f32) + 1. <= top)
        };
        found.map(|(_, id)| id.as_str())
    }
}

fn tone_of(item: &Suggestion, active: Option<&str>) -> MarkTone {
    match item {
        _ if Some(item.id.as_str()) == active => MarkTone::Active,
        Suggestion::Failed(_) => MarkTone::Failed,
        Suggestion::Ready(ready) if matches!(ready.refine, RefineState::Failed { .. }) => {
            MarkTone::Failed
        }
        _ => MarkTone::Suggestion,
    }
}

impl EditorPane {
    /// Notes an edit for the map, and redraws it when the typing stops.
    pub(crate) fn map_edited(&mut self, cx: &mut Context<Self>) {
        self.text_map.edited(Instant::now());
        self.text_map.settle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(MAP_SETTLE).await;
            let Ok(read) = this.update(cx, |this, cx| {
                (this.text_map.revision, this.editor.read(cx).text().clone())
            }) else {
                return;
            };
            let (revision, text) = read;
            let shapes = cx
                .background_spawn(async move { line_shapes(&text.to_string()) })
                .await;
            this.update(cx, |this, cx| {
                if this.text_map.revision == revision {
                    this.text_map.shapes = Some((revision, shapes));
                }
                cx.notify();
            })
            .log_warn();
        }));
    }

    /// The Document's lines in display rows, measured again only when the text or the wrapping
    /// changed. While the writer types, a frame that only draws keeps the lines measured
    /// before, so a keystroke does not measure the whole Document.
    fn measured_lines(&self, kind: Lines, cx: &App) -> Rc<[MapLine]> {
        perf_span!("text_map.measured_lines");
        let state = self.editor.read(cx);
        let display = state.display_map();
        let key = LinesKey {
            revision: self.text_map.revision,
            len: display.text().len(),
            wrap_rows: display.wrap_row_count(),
            width: self.pane_bounds().size.width,
        };
        let mut cache = self.text_map.lines.borrow_mut();
        let typing = self.text_map.typing(Instant::now());
        if let Some((cached, lines)) = cache.as_ref()
            && cached.fits(&key, kind, typing, display.has_estimated_rows())
        {
            return Rc::clone(lines);
        }
        let rows_of = |line: usize| {
            display
                .buffer_line_to_display_row_range(line)
                .map_or(0, |rows| rows.len())
        };
        let shapes = self
            .text_map
            .shapes
            .as_ref()
            .filter(|(revision, shapes)| {
                *revision == self.text_map.revision
                    && shapes
                        .last()
                        .is_some_and(|last| last.start + last.len == key.len)
            })
            .map(|(_, shapes)| shapes.clone());
        let lines: Rc<[MapLine]> = match shapes {
            Some(shapes) => fill_rows(shapes, rows_of),
            None => map_lines(&state.text().to_string(), rows_of),
        }
        .into();
        *cache = Some((key, Rc::clone(&lines)));
        lines
    }

    /// The Document's lines in display rows, the rows on screen, and the row height.
    fn rows_on_screen(&self, kind: Lines, cx: &App) -> Option<(Rc<[MapLine]>, View, Pixels)> {
        let line_height = self.editor.read(cx).line_height()?;
        let lines = self.measured_lines(kind, cx);
        let state = self.editor.read(cx);
        let view = View {
            first_row: f32::from(-state.scroll_offset().y) / f32::from(line_height),
            rows: f32::from(self.pane_bounds().size.height) / f32::from(line_height),
        };
        Some((lines, view, line_height))
    }

    /// The display row an offset is drawn on, after wrapping and folds.
    fn display_row(&self, lines: &[MapLine], offset: usize, cx: &App) -> usize {
        let (line, col) = locate(lines, offset);
        self.editor
            .read(cx)
            .display_map()
            .buffer_pos_to_display_pos(BufferPoint::new(line, col))
            .row
    }

    /// Scrolls so that a caret just moved to `offset` by a jump sits well inside the screen.
    pub(crate) fn reveal_jump(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some((lines, view, line_height)) = self.rows_on_screen(Lines::Exact, cx) else {
            return;
        };
        if let Some(first) = reveal_first_row(view, self.display_row(&lines, offset, cx)) {
            self.editor.update(cx, |state, cx| {
                state.set_scroll_offset(point(px(0.), -(line_height * first)), cx);
            });
        }
    }

    fn map_frame(&self, kind: Lines, cx: &App) -> Option<MapFrame> {
        let state = self.editor.read(cx);
        let (lines, view, line_height) = self.rows_on_screen(kind, cx)?;
        let active = active_suggestion(&self.suggestions, Some(state.cursor()))
            .map(|ready| ready.id.clone());
        let mut items: Vec<&Suggestion> = self.suggestions.items.iter().collect();
        items.sort_by_key(|item| (item.from, item.to));
        let marks: Rc<[MapMark]> = items
            .iter()
            .map(|item| MapMark {
                row: self.display_row(&lines, item.from, cx),
                tone: tone_of(item, active.as_deref()),
            })
            .collect();
        let ids = items.iter().map(|item| item.id.clone()).collect();
        Some(MapFrame {
            lines,
            marks,
            ids,
            view,
            line_height,
        })
    }

    /// Where `Alt+↑` and `Alt+↓` go when the caret is off screen: the Suggestion the screen
    /// steps to, so the keys agree with the chips. `None` when the caret is on screen.
    pub(crate) fn step_from_screen(&self, forward: bool, cx: &App) -> Option<usize> {
        let frame = self.map_frame(Lines::Exact, cx)?;
        let caret = self.display_row(&frame.lines, self.editor.read(cx).cursor(), cx) as f32;
        let view = frame.view;
        if caret + 1. > view.first_row && caret < view.first_row + view.rows {
            return None;
        }
        let index = step_in_view(&frame.rows(), view, forward)?;
        suggestion_anchor(&self.suggestions, frame.ids.get(index)?)
    }

    /// The map's left edge in the pane, right of the text where the scrollbar was, when the pane
    /// has room for it. Without room the editor keeps its own scrollbar.
    pub(crate) fn map_left(&self) -> Option<Pixels> {
        let pane = self.pane_bounds();
        let left = self.content_right() - pane.origin.x + px(MAP_GAP);
        (left + px(MAP_WIDTH + MAP_INSET) <= pane.size.width).then_some(left)
    }

    fn render_text_map(
        &self,
        frame: &MapFrame,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let left = self.map_left()?;
        let pane = self.pane_bounds();
        let room = f32::from(pane.size.height) - 2. * MAP_INSET;
        let map = text_map(&frame.lines, &frame.marks, frame.view, room);
        let bars: Vec<Div> = map
            .bars
            .iter()
            .map(|bar| {
                ui::text_map_bar(
                    bar.tone,
                    px(bar.top),
                    px(bar.height),
                    px(MAP_WIDTH * bar.width),
                    roles,
                )
            })
            .collect();
        let band = ui::text_map_band(px(map.band_top), px(map.band_height), roles);
        let height = px(map.height);
        let origin_y = pane.origin.y + px(MAP_INSET);
        let click = Rc::new(MapClick {
            marks: Rc::clone(&frame.marks),
            ids: Rc::clone(&frame.ids),
            map,
            view_rows: frame.view.rows,
            line_height: frame.line_height,
        });
        let line_height = frame.line_height;
        let element = ui::text_map_frame("text-map", px(MAP_WIDTH), height)
            .absolute()
            .left(left)
            .top(px(MAP_INSET))
            .child(band)
            .children(bars)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let y = f32::from(event.position.y - origin_y);
                    this.on_map_click(&click, origin_y, y, window, cx);
                }),
            )
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                this.scroll_by(event.delta.pixel_delta(line_height).y, cx);
            }));
        Some(element.into_any_element())
    }

    /// A press on a Suggestion's bar opens it; a press anywhere else scrolls there and starts a
    /// drag that follows the pointer until the button comes up.
    fn on_map_click(
        &mut self,
        click: &Rc<MapClick>,
        origin_y: Pixels,
        y: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = mark_near(&click.marks, &click.map, y, MAP_REACH)
            && let Some(id) = click.ids.get(index)
        {
            self.go_to_suggestion(id, window, cx);
            return;
        }
        self.text_map.drag = Some((Rc::clone(click), origin_y));
        self.scroll_map_to(click, y, cx);
    }

    /// Follows a drag that started on the map, wherever over the pane the pointer goes.
    pub(crate) fn drag_map(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((click, origin_y)) = self.text_map.drag.clone() else {
            return;
        };
        if event.pressed_button == Some(MouseButton::Left) {
            self.scroll_map_to(&click, f32::from(event.position.y - origin_y), cx);
        } else {
            self.text_map.drag = None;
        }
    }

    pub(crate) fn end_map_drag(&mut self) {
        self.text_map.drag = None;
    }

    /// Wheel turns that arrive before the editor applied the last one add up, so none is lost.
    fn scroll_by(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        let wheel = &self.text_map.wheel_target;
        self.editor.update(cx, |state, cx| {
            let offset = state.scroll_offset();
            let target = (wheel.get().unwrap_or(offset.y) + delta).min(px(0.));
            wheel.set(Some(target));
            state.set_scroll_offset(point(offset.x, target), cx);
        });
        cx.notify();
    }

    /// The text map and the edge chips, with the editor's scrollbar hidden while the map shows.
    pub(crate) fn render_map_and_chips(
        &self,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.text_map.wheel_target.set(None);
        let frame = self.map_frame(Lines::Drawn, cx);
        let map = frame
            .as_ref()
            .and_then(|frame| self.render_text_map(frame, roles, cx));
        self.hide_scrollbar_under_map(map.is_some(), cx);
        let Some(frame) = frame else {
            return Vec::new();
        };
        map.into_iter()
            .chain(self.render_edge_chips(&frame, roles, cx))
            .collect()
    }

    /// While the map stands in for it, the editor draws no scrollbar of its own.
    fn hide_scrollbar_under_map(&self, shown: bool, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, _| {
            state.set_scrollbar_renderer(shown.then(|| {
                let empty: EditorScrollbarRenderer = Rc::new(|_, _, _| div().into_any_element());
                empty
            }));
        });
    }

    fn scroll_map_to(&mut self, click: &MapClick, y: f32, cx: &mut Context<Self>) {
        let row = scroll_row_for(&click.map, y, click.view_rows);
        self.editor.update(cx, |state, cx| {
            state.set_scroll_offset(point(px(0.), -(click.line_height * row)), cx);
        });
        cx.notify();
    }

    pub(crate) fn go_to_suggestion(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = suggestion_anchor(&self.suggestions, id) else {
            return;
        };
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(at..at, cx);
            state.focus(window, cx);
        });
        self.reveal_jump(at, cx);
        cx.notify();
    }

    fn render_edge_chips(
        &self,
        frame: &MapFrame,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (above, below) = off_screen(&frame.rows(), frame.view);
        let pane = self.pane_bounds();
        let left = self.content_left() - pane.origin.x;
        let width = self.content_right() - self.content_left();
        [
            (false, above, frame.nearest_past(false)),
            (true, below, frame.nearest_past(true)),
        ]
        .into_iter()
        .filter_map(|(below, count, target)| {
            let id = target?.to_string();
            let chip = ui::edge_chip(if below { "chip-below" } else { "chip-above" }, roles)
                .child(chip_label(count, below))
                .child(ui::chip_key(if below { "Alt+↓" } else { "Alt+↑" }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.go_to_suggestion(&id, window, cx);
                }));
            let row = div()
                .absolute()
                .left(left)
                .w(width)
                .flex()
                .justify_center()
                .child(chip);
            Some(if below {
                row.bottom(px(CHIP_INSET)).into_any_element()
            } else {
                row.top(px(CHIP_INSET)).into_any_element()
            })
        })
        .collect()
    }
}

struct MapClick {
    marks: Rc<[MapMark]>,
    ids: Rc<[String]>,
    map: TextMap,
    view_rows: f32,
    line_height: Pixels,
}

fn chip_label(count: usize, below: bool) -> String {
    let arrow = if below { "↓" } else { "↑" };
    let noun = if count == 1 {
        "suggestion"
    } else {
        "suggestions"
    };
    let side = if below { "below" } else { "above" };
    format!("{arrow} {count} {noun} {side}")
}

#[cfg(test)]
mod tests {
    use super::{Lines, LinesKey};
    use crate::gpui::px;

    fn key(wrap_rows: usize) -> LinesKey {
        LinesKey {
            revision: 1,
            len: 100,
            wrap_rows,
            width: px(800.),
        }
    }

    #[test]
    fn a_frame_that_only_draws_keeps_the_lines_while_the_rows_are_estimated() {
        assert!(key(10).fits(&key(12), Lines::Drawn, false, true));
    }

    #[test]
    fn the_rows_changing_means_new_lines_once_they_are_exact() {
        assert!(!key(10).fits(&key(12), Lines::Drawn, false, false));
    }

    #[test]
    fn an_exact_request_measures_again_even_while_the_rows_are_estimated() {
        assert!(!key(10).fits(&key(12), Lines::Exact, false, true));
    }

    #[test]
    fn an_edit_or_a_new_width_measures_again_while_the_rows_are_estimated() {
        let edited = LinesKey {
            len: 101,
            ..key(12)
        };
        let narrower = LinesKey {
            width: px(700.),
            ..key(12)
        };
        assert!(!key(10).fits(&edited, Lines::Drawn, false, true));
        assert!(!key(10).fits(&narrower, Lines::Drawn, false, true));
    }

    #[test]
    fn the_same_key_always_fits() {
        assert!(key(10).fits(&key(10), Lines::Exact, false, false));
    }
}
