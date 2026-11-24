use super::types::NoticeKind;

pub const fn notice_message(kind: NoticeKind) -> &'static str {
    match kind {
        NoticeKind::TextChanged => "Suggestion discarded: the text changed",
        NoticeKind::PartlyKept => "Some changes left out: you edited those lines meanwhile",
        NoticeKind::Replaced => "Suggestion discarded: replaced by your new request",
        NoticeKind::NoChange => "No change suggested",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_the_discard_notice_for_edited_text() {
        assert_eq!(
            notice_message(NoticeKind::TextChanged),
            "Suggestion discarded: the text changed"
        );
    }

    #[test]
    fn words_the_discard_notice_for_a_replaced_suggestion() {
        assert_eq!(
            notice_message(NoticeKind::Replaced),
            "Suggestion discarded: replaced by your new request"
        );
    }

    #[test]
    fn words_the_notice_for_an_unchanged_replacement() {
        assert_eq!(notice_message(NoticeKind::NoChange), "No change suggested");
    }
}
