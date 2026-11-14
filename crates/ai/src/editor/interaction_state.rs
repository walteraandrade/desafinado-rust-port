#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InteractionState {
    pub pointer_down: bool,
    pub focused: bool,
    pub hovered_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionEffect {
    Pointer(bool),
    Focus(bool),
    Hover(Option<String>),
}

pub fn apply_interaction(state: InteractionState, effect: InteractionEffect) -> InteractionState {
    match effect {
        InteractionEffect::Pointer(pointer_down) => InteractionState {
            pointer_down,
            ..state
        },
        InteractionEffect::Focus(focused) => InteractionState { focused, ..state },
        InteractionEffect::Hover(hovered_id) => InteractionState {
            hovered_id,
            ..state
        },
    }
}

pub fn reduce_interaction(
    state: InteractionState,
    effects: impl IntoIterator<Item = InteractionEffect>,
) -> InteractionState {
    effects.into_iter().fold(state, apply_interaction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_idle() {
        assert_eq!(
            InteractionState::default(),
            InteractionState {
                pointer_down: false,
                focused: false,
                hovered_id: None
            }
        );
    }

    #[test]
    fn folds_effects_in_order() {
        let state = reduce_interaction(
            InteractionState::default(),
            [
                InteractionEffect::Pointer(true),
                InteractionEffect::Focus(true),
                InteractionEffect::Hover(Some("a".into())),
                InteractionEffect::Hover(Some("b".into())),
                InteractionEffect::Pointer(false),
            ],
        );
        assert_eq!(
            state,
            InteractionState {
                pointer_down: false,
                focused: true,
                hovered_id: Some("b".into())
            }
        );
    }

    #[test]
    fn clears_the_hover() {
        let state = reduce_interaction(
            InteractionState::default(),
            [
                InteractionEffect::Hover(Some("a".into())),
                InteractionEffect::Hover(None),
            ],
        );
        assert_eq!(state.hovered_id, None);
    }
}
