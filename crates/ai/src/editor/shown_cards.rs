#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStatus {
    Pending,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Active,
    Hover,
    Failed,
}

pub fn shown_cards<'a>(
    items: impl IntoIterator<Item = (&'a str, CardStatus)>,
    active_id: Option<&str>,
    hovered_id: Option<&str>,
) -> Vec<(&'a str, CardKind)> {
    items
        .into_iter()
        .filter_map(|(id, status)| match status {
            CardStatus::Failed => Some((id, CardKind::Failed)),
            CardStatus::Ready if Some(id) == active_id => Some((id, CardKind::Active)),
            CardStatus::Ready if Some(id) == hovered_id => Some((id, CardKind::Hover)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: [(&str, CardStatus); 4] = [
        ("a", CardStatus::Ready),
        ("b", CardStatus::Ready),
        ("c", CardStatus::Pending),
        ("d", CardStatus::Failed),
    ];

    #[test]
    fn shows_only_the_active_ready_card_and_failed_ones() {
        assert_eq!(
            shown_cards(ITEMS, Some("b"), None),
            vec![("b", CardKind::Active), ("d", CardKind::Failed)]
        );
    }

    #[test]
    fn adds_a_hover_card_for_another_ready_suggestion() {
        assert_eq!(
            shown_cards(ITEMS, Some("b"), Some("a")),
            vec![
                ("a", CardKind::Hover),
                ("b", CardKind::Active),
                ("d", CardKind::Failed)
            ]
        );
    }

    #[test]
    fn hovering_the_active_suggestion_adds_no_second_card() {
        assert_eq!(
            shown_cards(ITEMS, Some("b"), Some("b")),
            vec![("b", CardKind::Active), ("d", CardKind::Failed)]
        );
    }

    #[test]
    fn pending_suggestions_have_no_card() {
        assert_eq!(
            shown_cards(ITEMS, Some("c"), Some("c")),
            vec![("d", CardKind::Failed)]
        );
    }
}
