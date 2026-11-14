use super::types::VersionId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryMode {
    Editing,
    Version { id: VersionId },
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryModeAction {
    Show { id: VersionId },
    Step { delta: i64 },
    ToggleFull,
    Close,
}

fn index_of(history: &[VersionId], id: VersionId) -> Option<usize> {
    history.iter().position(|version| *version == id)
}

pub fn next_history_mode(
    history: &[VersionId],
    mode: HistoryMode,
    action: HistoryModeAction,
) -> HistoryMode {
    match action {
        HistoryModeAction::Show { id } => {
            if mode == (HistoryMode::Version { id }) {
                HistoryMode::Editing
            } else {
                HistoryMode::Version { id }
            }
        }
        HistoryModeAction::Step { delta } => {
            let HistoryMode::Version { id } = mode else {
                return mode;
            };
            let Some(index) = index_of(history, id) else {
                return HistoryMode::Editing;
            };
            let last = history.len() as i64 - 1;
            let target = (index as i64 + delta).clamp(0, last);
            history
                .get(target as usize)
                .map_or(HistoryMode::Editing, |id| HistoryMode::Version { id: *id })
        }
        HistoryModeAction::ToggleFull => {
            if mode == HistoryMode::Full {
                HistoryMode::Editing
            } else {
                HistoryMode::Full
            }
        }
        HistoryModeAction::Close => HistoryMode::Editing,
    }
}

pub fn visible_mode(mode: HistoryMode, history: &[VersionId]) -> HistoryMode {
    match mode {
        HistoryMode::Version { id } if index_of(history, id).is_none() => HistoryMode::Editing,
        other => other,
    }
}

pub fn dot_target(
    history: &[VersionId],
    current: Option<VersionId>,
    key: &str,
) -> Option<VersionId> {
    if history.is_empty() || !matches!(key, "ArrowLeft" | "ArrowRight" | "Home" | "End") {
        return None;
    }
    let last = history.len() - 1;
    let index = current.and_then(|id| index_of(history, id)).unwrap_or(last);
    let target = match key {
        "ArrowLeft" => index.saturating_sub(1),
        "ArrowRight" => (index + 1).min(last),
        "Home" => 0,
        _ => last,
    };
    history.get(target).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HISTORY: [VersionId; 3] = [1, 2, 3];

    fn next(mode: HistoryMode, action: HistoryModeAction) -> HistoryMode {
        next_history_mode(&HISTORY, mode, action)
    }

    #[test]
    fn shows_a_version_and_hides_it_on_a_second_click() {
        let shown = next(HistoryMode::Editing, HistoryModeAction::Show { id: 2 });
        assert_eq!(shown, HistoryMode::Version { id: 2 });
        assert_eq!(
            next(shown, HistoryModeAction::Show { id: 2 }),
            HistoryMode::Editing
        );
        assert_eq!(
            next(shown, HistoryModeAction::Show { id: 3 }),
            HistoryMode::Version { id: 3 }
        );
    }

    #[test]
    fn steps_to_older_and_newer_versions_stopping_at_the_ends() {
        let step = |id, delta| {
            next(
                HistoryMode::Version { id },
                HistoryModeAction::Step { delta },
            )
        };
        assert_eq!(step(2, -1), HistoryMode::Version { id: 1 });
        assert_eq!(step(1, -1), HistoryMode::Version { id: 1 });
        assert_eq!(step(3, 1), HistoryMode::Version { id: 3 });
    }

    #[test]
    fn goes_back_to_the_editor_when_stepping_from_a_version_that_is_gone() {
        assert_eq!(
            next(
                HistoryMode::Version { id: 99 },
                HistoryModeAction::Step { delta: -1 }
            ),
            HistoryMode::Editing
        );
    }

    #[test]
    fn ignores_a_step_outside_the_version_view() {
        assert_eq!(
            next(HistoryMode::Full, HistoryModeAction::Step { delta: 1 }),
            HistoryMode::Full
        );
    }

    #[test]
    fn toggles_the_full_history_from_anywhere() {
        assert_eq!(
            next(HistoryMode::Editing, HistoryModeAction::ToggleFull),
            HistoryMode::Full
        );
        assert_eq!(
            next(
                HistoryMode::Version { id: 1 },
                HistoryModeAction::ToggleFull
            ),
            HistoryMode::Full
        );
        assert_eq!(
            next(HistoryMode::Full, HistoryModeAction::ToggleFull),
            HistoryMode::Editing
        );
    }

    #[test]
    fn closes_back_to_the_editor() {
        assert_eq!(
            next(HistoryMode::Full, HistoryModeAction::Close),
            HistoryMode::Editing
        );
    }

    #[test]
    fn falls_back_to_the_editor_when_the_shown_version_is_gone() {
        assert_eq!(
            visible_mode(HistoryMode::Version { id: 99 }, &HISTORY),
            HistoryMode::Editing
        );
        let shown = HistoryMode::Version { id: 2 };
        assert_eq!(visible_mode(shown, &HISTORY), shown);
    }

    #[test]
    fn moves_with_the_arrow_keys_home_and_end() {
        assert_eq!(dot_target(&HISTORY, Some(2), "ArrowLeft"), Some(1));
        assert_eq!(dot_target(&HISTORY, Some(2), "ArrowRight"), Some(3));
        assert_eq!(dot_target(&HISTORY, Some(3), "ArrowRight"), Some(3));
        assert_eq!(dot_target(&HISTORY, Some(2), "Home"), Some(1));
        assert_eq!(dot_target(&HISTORY, Some(1), "End"), Some(3));
    }

    #[test]
    fn starts_from_the_newest_version_when_none_is_shown() {
        assert_eq!(dot_target(&HISTORY, None, "ArrowLeft"), Some(2));
    }

    #[test]
    fn moves_from_the_newest_version_when_the_current_one_is_gone() {
        assert_eq!(dot_target(&HISTORY, Some(99), "ArrowLeft"), Some(2));
    }

    #[test]
    fn returns_none_for_other_keys() {
        assert_eq!(dot_target(&HISTORY, Some(2), "Enter"), None);
    }
}
