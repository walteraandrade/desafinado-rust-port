use desafinado_ai::editor::place::{
    PlaceInput, PlaceMode, PlaceSelection, RestoredPlace, SavedPlace, ScrollTop, take_place,
};
use gpui_kit::*;

use crate::{comment_review::ReviewModeState, editor_pane::EditorPane, persist::PlaceSnapshot};

fn highest_id(ids: impl Iterator<Item = String>, prefix: char) -> usize {
    ids.filter_map(|id| id.strip_prefix(prefix)?.parse().ok())
        .max()
        .unwrap_or(0)
}

impl EditorPane {
    /// Where the writer is now: the selection, the scroll and the AI's open work.
    pub fn place(&self, cx: &App) -> SavedPlace {
        let PlaceSnapshot { text, rest } = self.place_snapshot(cx);
        SavedPlace {
            text: text.to_string(),
            ..rest
        }
    }

    /// The place without copying the text: the text is a Rope, the rest is small.
    pub(crate) fn place_snapshot(&self, cx: &App) -> PlaceSnapshot {
        let state = self.editor.read(cx);
        let range = state.selected_range();
        let rest = take_place(&PlaceInput {
            text: "",
            selection: PlaceSelection {
                start: range.start,
                end: range.end,
                reversed: state.is_selection_reversed(),
            },
            scroll_y: f32::from(-state.scroll_offset().y),
            scroll_top: state.scroll_top_line().map(|(line, within)| ScrollTop {
                line,
                within: f32::from(within),
            }),
            suggestions: &self.suggestions,
            notes: self.notes.as_ref(),
            note_rewrites: &self.note_rewrites,
            mode: self.mode.as_ref().map(|mode| PlaceMode {
                choice: mode.choice,
                focus: mode.focus.clone(),
            }),
        });
        PlaceSnapshot {
            text: state.text().clone(),
            rest,
        }
    }

    /// Puts back a place taken with [`Self::place`] over the text just set, without opening
    /// the Ask bubble over a restored selection.
    pub fn restore_place(&mut self, place: RestoredPlace, cx: &mut Context<Self>) {
        let ids = || place.suggestions.items.iter().map(|item| item.id.clone());
        self.next_id = self.next_id.max(highest_id(ids(), 's'));
        self.next_review_id = self.next_review_id.max(highest_id(ids(), 'r'));
        self.suggestions = place.suggestions;
        self.notes = place.notes;
        self.note_rewrites = place.note_rewrites;
        self.mode = place.mode.map(|mode| ReviewModeState {
            choice: mode.choice,
            focus: mode.focus,
        });
        let PlaceSelection {
            start,
            end,
            reversed,
        } = place.selection;
        self.editor.update(cx, |state, cx| {
            state.set_selected_range_with_direction(start..end, reversed, cx);
            match place.scroll_top {
                Some(top) => state.set_scroll_top_line(top.line, px(top.within), cx),
                None => {
                    state.set_scroll_offset_exact(point(px(0.), px(-place.scroll_y)), cx);
                }
            }
        });
        self.last_selection = self.editor.read(cx).selected_range();
        self.refresh_decorations(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::highest_id;

    #[test]
    fn new_ids_start_after_the_highest_restored_one_of_their_kind() {
        let ids = ["s2", "r7", "s11", "note"].map(String::from);
        assert_eq!(highest_id(ids.clone().into_iter(), 's'), 11);
        assert_eq!(highest_id(ids.into_iter(), 'r'), 7);
        assert_eq!(highest_id(std::iter::empty(), 's'), 0);
    }
}
