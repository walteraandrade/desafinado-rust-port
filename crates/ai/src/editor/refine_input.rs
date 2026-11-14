#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefineInputChange {
    pub opened: Option<String>,
    pub closed: bool,
    pub moved: bool,
}

pub fn next_refine_input(
    value: Option<&str>,
    change: &RefineInputChange,
    can_refine: impl Fn(&str) -> bool,
) -> Option<String> {
    let next = if change.closed {
        None
    } else if let Some(opened) = change.opened.as_deref() {
        Some(opened)
    } else if change.moved {
        None
    } else {
        value
    };
    next.filter(|id| can_refine(id)).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(id: &str) -> bool {
        id == "a"
    }

    fn open() -> RefineInputChange {
        RefineInputChange {
            opened: Some("a".into()),
            ..RefineInputChange::default()
        }
    }

    #[test]
    fn opens_for_a_ready_suggestion_and_closes_on_request() {
        let opened = next_refine_input(None, &open(), ready);
        assert_eq!(opened.as_deref(), Some("a"));
        let closed = next_refine_input(
            opened.as_deref(),
            &RefineInputChange {
                closed: true,
                ..RefineInputChange::default()
            },
            ready,
        );
        assert_eq!(closed, None);
    }

    #[test]
    fn does_not_open_for_unknown_or_pending_suggestions() {
        let unknown = RefineInputChange {
            opened: Some("missing".into()),
            ..RefineInputChange::default()
        };
        assert_eq!(next_refine_input(None, &unknown, ready), None);
        assert_eq!(next_refine_input(None, &open(), |_| false), None);
    }

    #[test]
    fn stays_open_while_the_cursor_and_the_document_stay_put() {
        assert_eq!(
            next_refine_input(Some("a"), &RefineInputChange::default(), ready).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn closes_when_the_cursor_moves_or_the_document_changes() {
        let moved = RefineInputChange {
            moved: true,
            ..RefineInputChange::default()
        };
        assert_eq!(next_refine_input(Some("a"), &moved, ready), None);
    }

    #[test]
    fn closes_when_the_refine_starts_or_the_suggestion_goes_away() {
        assert_eq!(
            next_refine_input(Some("a"), &RefineInputChange::default(), |_| false),
            None
        );
    }

    #[test]
    fn an_open_request_wins_over_a_move_in_the_same_transaction() {
        let both = RefineInputChange {
            opened: Some("a".into()),
            moved: true,
            ..RefineInputChange::default()
        };
        assert_eq!(next_refine_input(None, &both, ready).as_deref(), Some("a"));
    }

    #[test]
    fn opens_again_after_a_refine_failure_makes_the_suggestion_ready() {
        let refining = |_: &str| false;
        assert_eq!(next_refine_input(None, &open(), refining), None);
        assert_eq!(
            next_refine_input(None, &open(), ready).as_deref(),
            Some("a")
        );
    }
}
