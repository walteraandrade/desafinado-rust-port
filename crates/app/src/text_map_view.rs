//! Suggestions past the edges of the screen: the text map beside the column, and a chip at the
//! top and the foot of the text that counts them and goes to the nearest one.

use desafinado_core::{
    suggestions::{RefineState, Suggestion, active_suggestion, suggestion_anchor},
    text_map::{
        MapLine, MapMark, MarkTone, TextMap, View, map_lines, mark_near, off_screen,
        reveal_first_row, row_of_offset, scroll_row_for, text_map,
    },
};
use gpui_kit::*;

use crate::{editor_pane::EditorPane, theme::Roles, ui};

const MAP_WIDTH: f32 = 48.;
/// Room left of the column for the Notes' marks, and the gap before the map.
const NOTE_MARKS: f32 = 32.;
const MAP_GAP: f32 = 16.;
const MAP_INSET: f32 = 16.;
const CHIP_INSET: f32 = 8.;
/// How close a click on the map must land to a Suggestion's bar to open it.
const MAP_REACH: f32 = 6.;

/// What the map and the chips read from the editor on one frame.
pub(crate) struct MapFrame {
    lines: Vec<MapLine>,
    marks: Vec<MapMark>,
    ids: Vec<String>,
    view: View,
    line_height: Pixels,
}

impl MapFrame {
    fn rows(&self) -> Vec<usize> {
        self.marks
            .iter()
            .map(|mark| row_of_offset(&self.lines, mark.offset))
            .collect()
    }

    /// The Suggestion nearest the top edge of the screen from above, or the bottom edge from below.
    fn nearest_past(&self, below: bool) -> Option<&str> {
        let rows = self.rows();
        let top = self.view.first_row;
        let bottom = top + self.view.rows;
        let mut past = rows.iter().zip(&self.ids);
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
    /// The Document's lines in display rows, the rows on screen, and the row height.
    fn rows_on_screen(&self, cx: &App) -> Option<(Vec<MapLine>, View, Pixels)> {
        let state = self.editor.read(cx);
        let line_height = state.line_height()?;
        let display = state.display_map();
        let lines = map_lines(&state.value(), |line| {
            display
                .buffer_line_to_display_row_range(line)
                .map_or(0, |rows| rows.len())
        });
        let view = View {
            first_row: f32::from(-state.scroll_offset().y) / f32::from(line_height),
            rows: f32::from(self.pane_bounds().size.height) / f32::from(line_height),
        };
        Some((lines, view, line_height))
    }

    /// Scrolls so that a caret just moved to `offset` by a jump sits well inside the screen.
    pub(crate) fn reveal_jump(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some((lines, view, line_height)) = self.rows_on_screen(cx) else {
            return;
        };
        if let Some(first) = reveal_first_row(view, row_of_offset(&lines, offset)) {
            self.editor.update(cx, |state, cx| {
                state.set_scroll_offset(point(px(0.), -(line_height * first)), cx);
            });
        }
    }

    pub(crate) fn map_frame(&self, cx: &App) -> Option<MapFrame> {
        if self.suggestions.items.is_empty() {
            return None;
        }
        let state = self.editor.read(cx);
        let (lines, view, line_height) = self.rows_on_screen(cx)?;
        let active = active_suggestion(&self.suggestions, Some(state.cursor()))
            .map(|ready| ready.id.clone());
        let mut items: Vec<&Suggestion> = self.suggestions.items.iter().collect();
        items.sort_by_key(|item| (item.from, item.to));
        let marks = items
            .iter()
            .map(|item| MapMark {
                offset: item.from,
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

    /// The map's left edge in the pane, when the margin left of the column has room for it.
    fn map_left(&self) -> Option<Pixels> {
        let pane = self.pane_bounds();
        let left = self.content_left() - pane.origin.x - px(NOTE_MARKS + MAP_GAP + MAP_WIDTH);
        (left >= px(MAP_INSET)).then_some(left)
    }

    pub(crate) fn render_text_map(
        &self,
        frame: &MapFrame,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let left = self.map_left()?;
        let pane = self.pane_bounds();
        let room = f32::from(pane.size.height) - 2. * MAP_INSET;
        let map = text_map(&frame.lines, &frame.marks, frame.view, room);
        let bars = map.bars.iter().map(|bar| {
            ui::text_map_bar(
                bar.tone,
                px(bar.top),
                px(bar.height),
                px(MAP_WIDTH * bar.width),
                roles,
            )
        });
        let origin_y = pane.origin.y + px(MAP_INSET);
        let view_rows = frame.view.rows;
        let line_height = frame.line_height;
        let lines = frame.lines.clone();
        let marks = frame.marks.clone();
        let ids = frame.ids.clone();
        let element = ui::text_map_frame("text-map", px(MAP_WIDTH), px(map.height))
            .absolute()
            .left(left)
            .top(px(MAP_INSET))
            .child(ui::text_map_band(
                px(map.band_top),
                px(map.band_height),
                roles,
            ))
            .children(bars)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                let y = f32::from(event.position().y - origin_y);
                this.on_map_click(
                    &MapClick {
                        lines: &lines,
                        marks: &marks,
                        ids: &ids,
                        map: &map,
                        view_rows,
                        line_height,
                    },
                    y,
                    window,
                    cx,
                );
            }));
        Some(element.into_any_element())
    }

    fn on_map_click(
        &mut self,
        click: &MapClick,
        y: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = mark_near(click.lines, click.marks, click.map, y, MAP_REACH)
            && let Some(id) = click.ids.get(index)
        {
            self.go_to_suggestion(id, window, cx);
            return;
        }
        let row = scroll_row_for(click.map, y, click.view_rows);
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

    pub(crate) fn render_edge_chips(
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

struct MapClick<'a> {
    lines: &'a [MapLine],
    marks: &'a [MapMark],
    ids: &'a [String],
    map: &'a TextMap,
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
