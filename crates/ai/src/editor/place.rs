//! Where the writer was in a Document: the selection, the scroll and the AI's open work
//! (Suggestions ready for review, Notes and the Review mode), kept per file so that closing
//! the app or opening another file and coming back puts them in the same place.

use desafinado_core::{
    blocks::find_frontmatter,
    change_set::{Assoc, changes_between},
    diff::adaptive_diff,
    suggestions::{
        ReadySuggestion, RefineState, RefineStep, Suggestion, SuggestionBase, SuggestionsState,
        map_suggestions,
    },
};
use serde::{Deserialize, Serialize};

use crate::{notes::NoteBoard, review_mode::ModeChoice};

/// Bumped when a saved place can no longer be read as it is; an older one is ignored.
pub const PLACE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PlaceSelection {
    pub start: usize,
    pub end: usize,
    /// The caret is at the start of the range.
    pub reversed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceMode {
    pub choice: ModeChoice,
    pub focus: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceSuggestion {
    pub base: SuggestionBase,
    pub replacement: String,
    pub rationale: String,
    pub history: Vec<RefineStep>,
    pub shown: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPlace {
    pub version: u32,
    /// The text the place was taken in, to follow it through changes made outside the app.
    pub text: String,
    pub selection: PlaceSelection,
    pub scroll_y: f32,
    #[serde(default)]
    pub suggestions: Vec<PlaceSuggestion>,
    #[serde(default)]
    pub notes: Option<NoteBoard>,
    /// Suggestions asked for from a Note, by Suggestion id and Note number.
    #[serde(default)]
    pub note_rewrites: Vec<(String, usize)>,
    #[serde(default)]
    pub mode: Option<PlaceMode>,
}

pub struct PlaceInput<'a> {
    pub text: &'a str,
    pub selection: PlaceSelection,
    pub scroll_y: f32,
    pub suggestions: &'a SuggestionsState,
    pub notes: Option<&'a NoteBoard>,
    pub note_rewrites: &'a [(String, usize)],
    pub mode: Option<PlaceMode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RestoredPlace {
    pub selection: PlaceSelection,
    pub scroll_y: f32,
    pub suggestions: SuggestionsState,
    pub notes: Option<NoteBoard>,
    pub note_rewrites: Vec<(String, usize)>,
    pub mode: Option<PlaceMode>,
    /// Suggestions that were saved but no longer fit the text.
    pub dropped: usize,
}

/// Takes the place to keep. Only Suggestions ready for review are kept: one still waiting for
/// the model, or one that failed, has nothing to show after a restart, and a Refine in
/// progress falls back to the version on screen.
pub fn take_place(input: &PlaceInput) -> SavedPlace {
    let suggestions: Vec<PlaceSuggestion> = input
        .suggestions
        .items
        .iter()
        .filter_map(Suggestion::as_ready)
        .map(|ready| PlaceSuggestion {
            base: ready.base.clone(),
            replacement: ready.replacement.clone(),
            rationale: ready.rationale.clone(),
            history: ready.history.clone(),
            shown: ready.shown,
        })
        .collect();
    let notes = input.notes.filter(|board| !board.is_empty()).cloned();
    let note_rewrites = kept_rewrites(
        input.note_rewrites,
        suggestions.iter().map(|kept| kept.base.id.as_str()),
        notes.as_ref(),
    );
    SavedPlace {
        version: PLACE_VERSION,
        text: input.text.to_string(),
        selection: input.selection,
        scroll_y: input.scroll_y,
        suggestions,
        notes,
        note_rewrites,
        mode: input.mode.clone(),
    }
}

fn kept_rewrites<'a>(
    rewrites: &[(String, usize)],
    ids: impl Iterator<Item = &'a str>,
    notes: Option<&NoteBoard>,
) -> Vec<(String, usize)> {
    let ids: Vec<&str> = ids.collect();
    rewrites
        .iter()
        .filter(|(id, number)| {
            ids.contains(&id.as_str()) && notes.is_some_and(|board| board.note(*number).is_some())
        })
        .cloned()
        .collect()
}

fn ready_of(saved: &PlaceSuggestion, text: &str) -> Option<Suggestion> {
    let base = &saved.base;
    let in_text = base.from <= base.to && text.get(base.from..base.to) == Some(&base.original);
    if !in_text || saved.shown >= saved.history.len() {
        return None;
    }
    Some(Suggestion::Ready(ReadySuggestion {
        base: base.clone(),
        diff: adaptive_diff(&base.original, &saved.replacement),
        replacement: saved.replacement.clone(),
        rationale: saved.rationale.clone(),
        history: saved.history.clone(),
        shown: saved.shown,
        refine: RefineState::Idle,
    }))
}

fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn still_fits(item: &Suggestion, text: &str) -> bool {
    item.from <= item.to && text.get(item.from..item.to) == Some(item.original.as_str())
}

/// Puts a saved place back over `text`, the Document as it opens now. When the file changed
/// since the place was taken, everything follows the change as it would follow an edit:
/// a Suggestion whose text was changed is dropped, and a Note over changed text is marked stale.
pub fn restore_place(saved: &SavedPlace, text: &str) -> Option<RestoredPlace> {
    if saved.version != PLACE_VERSION {
        return None;
    }
    let items: Vec<Suggestion> = saved
        .suggestions
        .iter()
        .filter_map(|kept| ready_of(kept, &saved.text))
        .collect();
    let next_sequence = items
        .iter()
        .map(|item| item.sequence + 1)
        .max()
        .unwrap_or(0);
    let state = SuggestionsState {
        items,
        next_sequence,
        notice: None,
    };
    let changes = (saved.text != text).then(|| changes_between(&saved.text, text));
    let (state, notes, selection) = match &changes {
        None => (state, saved.notes.clone(), saved.selection),
        Some(changes) => (
            map_suggestions(&state, changes, find_frontmatter(text).as_ref()),
            saved.notes.as_ref().map(|board| board.map_through(changes)),
            PlaceSelection {
                start: changes.map_pos(saved.selection.start, Assoc::After),
                end: changes.map_pos(saved.selection.end, Assoc::Before),
                reversed: saved.selection.reversed,
            },
        ),
    };
    let items: Vec<Suggestion> = state
        .items
        .into_iter()
        .filter(|item| still_fits(item, text))
        .collect();
    let notes = notes
        .map(|board| NoteBoard {
            notes: board
                .notes
                .iter()
                .filter(|note| note.from <= note.to && note.to <= text.len())
                .cloned()
                .collect(),
            ..board
        })
        .filter(|board| !board.is_empty());
    let note_rewrites = kept_rewrites(
        &saved.note_rewrites,
        items.iter().map(|item| item.id.as_str()),
        notes.as_ref(),
    );
    let start = floor_boundary(text, selection.start);
    let end = floor_boundary(text, selection.end.max(start));
    Some(RestoredPlace {
        selection: PlaceSelection {
            start,
            end,
            reversed: selection.reversed && start != end,
        },
        scroll_y: saved.scroll_y.max(0.),
        dropped: saved.suggestions.len() - items.len(),
        suggestions: SuggestionsState {
            items,
            next_sequence: state.next_sequence,
            notice: None,
        },
        notes,
        note_rewrites,
        mode: saved.mode.clone(),
    })
}

pub fn place_to_json(place: &SavedPlace) -> Result<String, serde_json::Error> {
    serde_json::to_string(place)
}

pub fn place_from_json(json: &str) -> Result<SavedPlace, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use desafinado_core::suggestions::{
        Instruction, SuggestionDraft, SuggestionOrigin, SuggestionResult, TargetSource,
        begin_refine, begin_suggestion, empty_suggestions, resolve_refine, resolve_suggestion,
    };

    use super::*;
    use crate::notes::{GeneralNote, Note, NoteKind};
    use crate::review_mode::{ModeSource, ReviewMode};

    const TEXT: &str = "First paragraph here.\n\nSecond paragraph, too long by far.\n";

    fn instruction(text: &str) -> Instruction {
        Instruction::FreeText {
            text: text.to_string(),
        }
    }

    fn with_ready(
        state: &SuggestionsState,
        id: &str,
        original: &str,
        replacement: &str,
    ) -> SuggestionsState {
        let from = TEXT.find(original).unwrap();
        let begun = begin_suggestion(
            state,
            SuggestionDraft {
                id: id.to_string(),
                from,
                to: from + original.len(),
                source: TargetSource::Block,
                original: original.to_string(),
                instruction: instruction("Shorter"),
                origin: Some(SuggestionOrigin::Ask),
            },
        );
        resolve_suggestion(
            &begun,
            id,
            &SuggestionResult {
                replacement: replacement.to_string(),
                rationale: "Tighter.".to_string(),
                unchanged: false,
            },
        )
    }

    fn pending(state: &SuggestionsState, id: &str, original: &str) -> SuggestionsState {
        let from = TEXT.find(original).unwrap();
        begin_suggestion(
            state,
            SuggestionDraft {
                id: id.to_string(),
                from,
                to: from + original.len(),
                source: TargetSource::Selection,
                original: original.to_string(),
                instruction: instruction("Clearer"),
                origin: None,
            },
        )
    }

    fn board() -> NoteBoard {
        let from = TEXT.find("Second").unwrap();
        NoteBoard {
            general: vec![GeneralNote {
                topic: "Shape".to_string(),
                text: "Reads well.".to_string(),
            }],
            notes: vec![Note {
                number: 1,
                block_id: "b2".to_string(),
                from,
                to: TEXT.len() - 1,
                kind: NoteKind::Clarity,
                title: "Long".to_string(),
                comment: "Cut it.".to_string(),
                stale: false,
            }],
            show_all: false,
            active: Some(1),
            dropped: 0,
        }
    }

    fn mode() -> PlaceMode {
        PlaceMode {
            choice: ModeChoice {
                mode: ReviewMode::Comment,
                source: ModeSource::Decided {
                    probability: Some(0.9),
                },
            },
            focus: "tone?".to_string(),
        }
    }

    fn taken(state: &SuggestionsState) -> SavedPlace {
        let board = board();
        take_place(&PlaceInput {
            text: TEXT,
            selection: PlaceSelection {
                start: 3,
                end: 8,
                reversed: true,
            },
            scroll_y: 120.,
            suggestions: state,
            notes: Some(&board),
            note_rewrites: &[("s2".to_string(), 1), ("s9".to_string(), 1)],
            mode: Some(mode()),
        })
    }

    #[test]
    fn the_same_text_gets_back_its_selection_scroll_suggestions_notes_and_mode() {
        let state = with_ready(
            &empty_suggestions(),
            "s2",
            "Second paragraph, too long by far.",
            "Second, shorter.",
        );
        let saved = taken(&state);
        let restored = restore_place(&saved, TEXT).unwrap();
        assert_eq!(
            restored.selection,
            PlaceSelection {
                start: 3,
                end: 8,
                reversed: true
            }
        );
        assert!((restored.scroll_y - 120.).abs() < f32::EPSILON);
        assert_eq!(restored.suggestions.items, state.items);
        assert_eq!(restored.suggestions.next_sequence, state.next_sequence);
        assert_eq!(restored.notes, Some(board()));
        assert_eq!(restored.note_rewrites, vec![("s2".to_string(), 1)]);
        assert_eq!(restored.mode, Some(mode()));
        assert_eq!(restored.dropped, 0);
    }

    #[test]
    fn keeps_only_suggestions_ready_for_review_and_ends_a_refine_in_progress() {
        let ready = with_ready(
            &empty_suggestions(),
            "s1",
            "First paragraph here.",
            "First.",
        );
        let refining = begin_refine(&ready, "s1", instruction("even shorter"));
        let state = pending(&refining, "s2", "Second paragraph");
        let saved = taken(&state);
        assert_eq!(saved.suggestions.len(), 1);
        let restored = restore_place(&saved, TEXT).unwrap();
        let item = restored.suggestions.items[0].as_ready().unwrap();
        assert_eq!(item.id, "s1");
        assert_eq!(item.refine, RefineState::Idle);
        assert_eq!(item.replacement, "First.");
    }

    #[test]
    fn keeps_every_refined_version_and_the_one_on_screen() {
        let ready = with_ready(
            &empty_suggestions(),
            "s1",
            "First paragraph here.",
            "First.",
        );
        let refining = begin_refine(&ready, "s1", instruction("formal"));
        let refined = resolve_refine(
            &refining,
            "s1",
            &SuggestionResult {
                replacement: "The first paragraph.".to_string(),
                rationale: "Formal.".to_string(),
                unchanged: false,
            },
        );
        let restored = restore_place(&taken(&refined), TEXT).unwrap();
        let item = restored.suggestions.items[0].as_ready().unwrap();
        assert_eq!(item.history.len(), 2);
        assert_eq!(item.shown, 1);
        assert_eq!(item.replacement, "The first paragraph.");
        assert_eq!(
            item.diff,
            adaptive_diff("First paragraph here.", "The first paragraph.")
        );
    }

    #[test]
    fn a_change_outside_the_app_moves_what_it_does_not_touch_and_drops_what_it_does() {
        let state = with_ready(
            &empty_suggestions(),
            "s1",
            "First paragraph here.",
            "First.",
        );
        let state = with_ready(
            &state,
            "s2",
            "Second paragraph, too long by far.",
            "Second.",
        );
        let saved = taken(&state);
        let changed = TEXT.replace("Second paragraph", "Another paragraph");
        let changed = format!("Title\n\n{changed}");
        let restored = restore_place(&saved, &changed).unwrap();
        assert_eq!(restored.dropped, 1);
        let ids: Vec<&str> = restored
            .suggestions
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(ids, vec!["s1"]);
        let first = &restored.suggestions.items[0];
        assert_eq!(&changed[first.from..first.to], "First paragraph here.");
        assert_eq!(restored.selection.start, 3 + "Title\n\n".len());
        let notes = restored.notes.unwrap();
        assert!(notes.notes[0].stale);
        assert!(restored.note_rewrites.is_empty());
    }

    #[test]
    fn a_shorter_text_keeps_the_selection_inside_it_and_on_a_character() {
        let saved = SavedPlace {
            selection: PlaceSelection {
                start: 40,
                end: 50,
                reversed: true,
            },
            ..taken(&empty_suggestions())
        };
        let restored = restore_place(&saved, "é").unwrap();
        assert!(restored.selection.end <= "é".len());
        assert!("é".is_char_boundary(restored.selection.start));
        assert!("é".is_char_boundary(restored.selection.end));
    }

    #[test]
    fn drops_a_suggestion_whose_saved_range_does_not_hold_its_original() {
        let mut saved = taken(&with_ready(
            &empty_suggestions(),
            "s1",
            "First paragraph here.",
            "First.",
        ));
        saved.suggestions[0].base.from += 1;
        let restored = restore_place(&saved, TEXT).unwrap();
        assert!(restored.suggestions.items.is_empty());
        assert_eq!(restored.dropped, 1);
    }

    #[test]
    fn ignores_a_place_saved_in_another_version() {
        let saved = SavedPlace {
            version: PLACE_VERSION + 1,
            ..taken(&empty_suggestions())
        };
        assert_eq!(restore_place(&saved, TEXT), None);
    }

    #[test]
    fn survives_a_round_trip_through_json() {
        let state = with_ready(
            &empty_suggestions(),
            "s2",
            "Second paragraph, too long by far.",
            "Second.",
        );
        let saved = taken(&state);
        let json = place_to_json(&saved).unwrap();
        assert_eq!(place_from_json(&json).unwrap(), saved);
    }
}
