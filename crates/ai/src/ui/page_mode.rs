#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryMode {
    Editing,
    Version { id: u64 },
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryModeAction {
    Show { id: u64 },
    Step { delta: i64 },
    ToggleFull,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageMode {
    History(HistoryMode),
    Evals,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageAction {
    History(HistoryModeAction),
    ToggleEvals,
    OpenEvals,
}

pub const EDITING: PageMode = PageMode::History(HistoryMode::Editing);

fn index_of(history: &[u64], id: u64) -> Option<usize> {
    history.iter().position(|version| *version == id)
}

pub fn next_history_mode(
    history: &[u64],
    mode: &HistoryMode,
    action: &HistoryModeAction,
) -> HistoryMode {
    match action {
        HistoryModeAction::Show { id } => match mode {
            HistoryMode::Version { id: current } if current == id => HistoryMode::Editing,
            _ => HistoryMode::Version { id: *id },
        },
        HistoryModeAction::Step { delta } => {
            let HistoryMode::Version { id } = mode else {
                return mode.clone();
            };
            let Some(index) = index_of(history, *id) else {
                return HistoryMode::Editing;
            };
            let last = history.len() as i64 - 1;
            let target = (index as i64 + delta).clamp(0, last);
            history
                .get(target as usize)
                .map_or(HistoryMode::Editing, |id| HistoryMode::Version { id: *id })
        }
        HistoryModeAction::ToggleFull => {
            if *mode == HistoryMode::Full {
                HistoryMode::Editing
            } else {
                HistoryMode::Full
            }
        }
        HistoryModeAction::Close => HistoryMode::Editing,
    }
}

pub fn visible_mode(mode: &HistoryMode, history: &[u64]) -> HistoryMode {
    match mode {
        HistoryMode::Version { id } if index_of(history, *id).is_none() => HistoryMode::Editing,
        other => other.clone(),
    }
}

pub fn next_page_mode(history: &[u64], mode: &PageMode, action: &PageAction) -> PageMode {
    match (action, mode) {
        (PageAction::ToggleEvals, PageMode::Evals) => EDITING,
        (PageAction::ToggleEvals | PageAction::OpenEvals, _) => PageMode::Evals,
        (PageAction::History(HistoryModeAction::Step { .. }), PageMode::Evals) => mode.clone(),
        (PageAction::History(action), PageMode::Evals) => {
            PageMode::History(next_history_mode(history, &HistoryMode::Editing, action))
        }
        (PageAction::History(action), PageMode::History(current)) => {
            PageMode::History(next_history_mode(history, current, action))
        }
    }
}

pub fn visible_page(mode: &PageMode, history: &[u64], evals_available: bool) -> PageMode {
    match mode {
        PageMode::Evals if evals_available => PageMode::Evals,
        PageMode::Evals => EDITING,
        PageMode::History(history_mode) => PageMode::History(visible_mode(history_mode, history)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HISTORY: [u64; 2] = [1, 2];

    fn next(mode: &PageMode, action: PageAction) -> PageMode {
        next_page_mode(&HISTORY, mode, &action)
    }

    fn version(id: u64) -> PageMode {
        PageMode::History(HistoryMode::Version { id })
    }

    fn full() -> PageMode {
        PageMode::History(HistoryMode::Full)
    }

    fn history(action: HistoryModeAction) -> PageAction {
        PageAction::History(action)
    }

    #[test]
    fn opens_the_evals_page_from_the_text() {
        assert_eq!(next(&EDITING, PageAction::ToggleEvals), PageMode::Evals);
    }

    #[test]
    fn closes_the_evals_page_when_its_button_is_pressed_again() {
        assert_eq!(next(&PageMode::Evals, PageAction::ToggleEvals), EDITING);
    }

    #[test]
    fn opens_the_evals_page_and_keeps_it_open_when_asked_again() {
        assert_eq!(next(&EDITING, PageAction::OpenEvals), PageMode::Evals);
        assert_eq!(
            next(&PageMode::Evals, PageAction::OpenEvals),
            PageMode::Evals
        );
        assert_eq!(next(&full(), PageAction::OpenEvals), PageMode::Evals);
    }

    #[test]
    fn opening_the_evals_page_closes_a_version_and_the_full_history() {
        assert_eq!(next(&version(1), PageAction::ToggleEvals), PageMode::Evals);
        assert_eq!(next(&full(), PageAction::ToggleEvals), PageMode::Evals);
    }

    #[test]
    fn opening_a_version_or_the_full_history_closes_the_evals_page() {
        assert_eq!(
            next(&PageMode::Evals, history(HistoryModeAction::Show { id: 2 })),
            version(2)
        );
        assert_eq!(
            next(&PageMode::Evals, history(HistoryModeAction::ToggleFull)),
            full()
        );
    }

    #[test]
    fn goes_back_to_the_text_on_close() {
        assert_eq!(
            next(&PageMode::Evals, history(HistoryModeAction::Close)),
            EDITING
        );
    }

    #[test]
    fn ignores_a_step_outside_the_version_view() {
        assert_eq!(
            next(
                &PageMode::Evals,
                history(HistoryModeAction::Step { delta: 1 })
            ),
            PageMode::Evals
        );
    }

    #[test]
    fn keeps_the_history_transitions() {
        assert_eq!(
            next(&EDITING, history(HistoryModeAction::Show { id: 1 })),
            version(1)
        );
        assert_eq!(
            next(&version(1), history(HistoryModeAction::Step { delta: 1 })),
            version(2)
        );
    }

    #[test]
    fn showing_the_open_version_again_returns_to_the_text() {
        assert_eq!(
            next(&version(1), history(HistoryModeAction::Show { id: 1 })),
            EDITING
        );
    }

    #[test]
    fn clamps_steps_at_both_ends_and_drops_a_missing_version() {
        assert_eq!(
            next(&version(2), history(HistoryModeAction::Step { delta: 5 })),
            version(2)
        );
        assert_eq!(
            next(&version(1), history(HistoryModeAction::Step { delta: -5 })),
            version(1)
        );
        assert_eq!(
            next(&version(9), history(HistoryModeAction::Step { delta: 1 })),
            EDITING
        );
    }

    #[test]
    fn falls_back_to_the_text_when_the_shown_version_is_gone() {
        assert_eq!(visible_page(&version(99), &HISTORY, true), EDITING);
    }

    #[test]
    fn falls_back_to_the_text_when_the_open_file_has_no_evals_page() {
        assert_eq!(visible_page(&PageMode::Evals, &HISTORY, false), EDITING);
    }

    #[test]
    fn keeps_the_evals_page_when_it_is_available() {
        assert_eq!(
            visible_page(&PageMode::Evals, &HISTORY, true),
            PageMode::Evals
        );
    }
}
