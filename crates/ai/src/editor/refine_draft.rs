#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineDraft {
    pub owner: String,
    pub value: String,
    pub focused: bool,
}

pub fn restorable_draft<'a>(
    draft: Option<&'a RefineDraft>,
    owner: Option<&str>,
) -> Option<&'a RefineDraft> {
    match (draft, owner) {
        (Some(draft), Some(owner)) if draft.owner == owner => Some(draft),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> RefineDraft {
        RefineDraft {
            owner: "a".into(),
            value: "make it shor".into(),
            focused: true,
        }
    }

    #[test]
    fn gives_back_the_draft_for_the_suggestion_whose_input_is_open() {
        let draft = draft();
        assert_eq!(restorable_draft(Some(&draft), Some("a")), Some(&draft));
    }

    #[test]
    fn drops_the_draft_when_another_suggestion_or_none_owns_the_input() {
        let draft = draft();
        assert_eq!(restorable_draft(Some(&draft), Some("b")), None);
        assert_eq!(restorable_draft(Some(&draft), None), None);
        assert_eq!(restorable_draft(None, Some("a")), None);
    }
}
