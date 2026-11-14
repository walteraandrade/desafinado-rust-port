use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::model::{Block, BlockKind};
use crate::schemas::{ReviewEdit, SuggestionOutput};
use crate::text::js_trim;

pub const MAX_REVIEW_EDITS: usize = 6;
pub const JUNK_MIN_ORIGINAL_WORDS: usize = 15;
pub const JUNK_MAX_REPLACEMENT_WORDS: usize = 3;
pub const UNRELATED_MIN_ORIGINAL_WORDS: usize = 8;

static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]+(?:['’-][\p{L}\p{N}]+)*").expect("valid regex"));

static CHATTY_OPENER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(sure|certainly|of course|here is|here's|i'?m sorry|i cannot|i can'?t|as an ai|thank you|thanks)\b",
    )
    .expect("valid regex")
});

fn word_count(text: &str) -> usize {
    WORD.find_iter(text).count()
}

fn content_words(text: &str) -> BTreeSet<String> {
    WORD.find_iter(text)
        .map(|word| word.as_str().to_lowercase())
        .filter(|word| word.encode_utf16().count() >= 4)
        .collect()
}

fn has_chatty_opener(original: &str, replacement: &str) -> bool {
    CHATTY_OPENER.is_match(replacement) && !CHATTY_OPENER.is_match(original)
}

fn shares_no_content(original: &str, replacement: &str) -> bool {
    let kept = content_words(original);
    let proposed = content_words(replacement);
    word_count(original) >= UNRELATED_MIN_ORIGINAL_WORDS
        && !kept.is_empty()
        && !proposed.is_empty()
        && proposed.iter().all(|word| !kept.contains(word))
}

pub fn is_junk_replacement(original: &str, replacement: &str) -> bool {
    (word_count(original) > JUNK_MIN_ORIGINAL_WORDS
        && word_count(replacement) < JUNK_MAX_REPLACEMENT_WORDS)
        || has_chatty_opener(original, replacement)
        || shares_no_content(original, replacement)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DropReason {
    UnknownBlock,
    CodeBlock,
    Occupied,
    DuplicateBlock,
    Empty,
    Unchanged,
    Junk,
    OverLimit,
}

impl DropReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownBlock => "unknown-block",
            Self::CodeBlock => "code-block",
            Self::Occupied => "occupied",
            Self::DuplicateBlock => "duplicate-block",
            Self::Empty => "empty",
            Self::Unchanged => "unchanged",
            Self::Junk => "junk",
            Self::OverLimit => "over-limit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedEdit {
    pub block_id: String,
    pub replacement: String,
    pub rationale: String,
    pub block: Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedEdit {
    pub edit: ReviewEdit,
    pub reason: DropReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewValidationState {
    pub validated: usize,
    pub seen: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ReviewValidationContext {
    pub blocks: Vec<Block>,
    pub occupied: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutcome {
    Validated(ValidatedEdit),
    Dropped(DroppedEdit),
}

fn drop_reason(
    occupied: &BTreeSet<String>,
    state: &ReviewValidationState,
    edit: &ReviewEdit,
    block: Option<&Block>,
) -> Option<DropReason> {
    let Some(block) = block else {
        return Some(DropReason::UnknownBlock);
    };
    if block.kind == BlockKind::Code {
        return Some(DropReason::CodeBlock);
    }
    if occupied.contains(&block.id) {
        return Some(DropReason::Occupied);
    }
    if state.seen.contains(&block.id) {
        return Some(DropReason::DuplicateBlock);
    }
    let replacement = js_trim(&edit.replacement);
    if replacement.is_empty() {
        return Some(DropReason::Empty);
    }
    if replacement == js_trim(&block.text) {
        return Some(DropReason::Unchanged);
    }
    if block.kind != BlockKind::Heading && is_junk_replacement(&block.text, replacement) {
        return Some(DropReason::Junk);
    }
    if state.validated >= MAX_REVIEW_EDITS {
        return Some(DropReason::OverLimit);
    }
    None
}

pub fn validate_review_edit(
    context: &ReviewValidationContext,
    state: &ReviewValidationState,
    edit: &ReviewEdit,
) -> (ReviewValidationState, ReviewOutcome) {
    let block = context
        .blocks
        .iter()
        .find(|block| block.id == edit.block_id);
    match (drop_reason(&context.occupied, state, edit, block), block) {
        (None, Some(block)) => {
            let mut seen = state.seen.clone();
            seen.insert(block.id.clone());
            (
                ReviewValidationState {
                    validated: state.validated + 1,
                    seen,
                },
                ReviewOutcome::Validated(ValidatedEdit {
                    block_id: edit.block_id.clone(),
                    replacement: edit.replacement.clone(),
                    rationale: edit.rationale.clone(),
                    block: block.clone(),
                }),
            )
        }
        (reason, _) => (
            state.clone(),
            ReviewOutcome::Dropped(DroppedEdit {
                edit: edit.clone(),
                reason: reason.unwrap_or(DropReason::UnknownBlock),
            }),
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewValidation {
    pub validated: Vec<ValidatedEdit>,
    pub dropped: Vec<DroppedEdit>,
}

pub fn validate_review_edits(
    edits: &[ReviewEdit],
    blocks: &[Block],
    occupied: &BTreeSet<String>,
) -> ReviewValidation {
    let context = ReviewValidationContext {
        blocks: blocks.to_vec(),
        occupied: occupied.clone(),
    };
    let (_, run) = edits.iter().fold(
        (
            ReviewValidationState::default(),
            ReviewValidation::default(),
        ),
        |(state, mut run), edit| {
            let (next, outcome) = validate_review_edit(&context, &state, edit);
            match outcome {
                ReviewOutcome::Validated(edit) => run.validated.push(edit),
                ReviewOutcome::Dropped(dropped) => run.dropped.push(dropped),
            }
            (next, run)
        },
    );
    run
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionResult {
    pub replacement: String,
    pub rationale: String,
    pub unchanged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidSuggestion {
    Empty,
}

pub fn check_suggestion(
    original: &str,
    output: &SuggestionOutput,
) -> Result<SuggestionResult, InvalidSuggestion> {
    let replacement = js_trim(&output.replacement);
    if replacement.is_empty() {
        return Err(InvalidSuggestion::Empty);
    }
    Ok(SuggestionResult {
        replacement: output.replacement.clone(),
        rationale: output.rationale.clone(),
        unchanged: replacement == js_trim(original),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{SAMPLE_DOCUMENT, extract_blocks};

    const DOC: &str = "# Title\n\nOne.\n\nTwo.\n\n```js\nconst a = 1\n```\n\nThree.\n\nFour.\n\nFive.\n\nSix.\n\nSeven.\n\nEight.\n";

    fn blocks() -> Vec<Block> {
        extract_blocks(DOC)
    }

    fn id_of(text: &str) -> String {
        blocks()
            .into_iter()
            .find(|block| block.text == text)
            .unwrap_or_else(|| panic!("no block {text}"))
            .id
    }

    fn edit_with(text: &str, replacement: &str) -> ReviewEdit {
        ReviewEdit {
            block_id: id_of(text),
            replacement: replacement.to_owned(),
            rationale: "Because.".into(),
        }
    }

    fn edit(text: &str) -> ReviewEdit {
        edit_with(text, &format!("{text} edited"))
    }

    fn raw(block_id: &str, replacement: &str, rationale: &str) -> ReviewEdit {
        ReviewEdit {
            block_id: block_id.into(),
            replacement: replacement.into(),
            rationale: rationale.into(),
        }
    }

    fn none() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn reasons(run: &ReviewValidation) -> Vec<DropReason> {
        run.dropped.iter().map(|d| d.reason).collect()
    }

    #[test]
    fn keeps_an_edit_that_changes_a_known_block() {
        let run = validate_review_edits(&[edit("One.")], &blocks(), &none());
        assert!(run.dropped.is_empty());
        assert_eq!(run.validated.len(), 1);
        assert_eq!(run.validated[0].block_id, id_of("One."));
        assert_eq!(run.validated[0].replacement, "One. edited");
        assert_eq!(run.validated[0].block.text, "One.");
    }

    #[test]
    fn drops_an_edit_with_an_unknown_block_id() {
        let run = validate_review_edits(&[raw("p99", "x", "y")], &blocks(), &none());
        assert!(run.validated.is_empty());
        assert_eq!(reasons(&run), [DropReason::UnknownBlock]);
    }

    #[test]
    fn drops_an_edit_whose_replacement_equals_the_original() {
        let run = validate_review_edits(&[edit_with("One.", "One.")], &blocks(), &none());
        assert_eq!(reasons(&run), [DropReason::Unchanged]);
    }

    #[test]
    fn compares_the_original_and_the_replacement_trimmed() {
        let run = validate_review_edits(&[edit_with("One.", "  One.\n")], &blocks(), &none());
        assert_eq!(reasons(&run), [DropReason::Unchanged]);
    }

    #[test]
    fn drops_an_edit_with_an_empty_replacement() {
        let run = validate_review_edits(&[edit_with("One.", "  ")], &blocks(), &none());
        assert_eq!(reasons(&run), [DropReason::Empty]);
    }

    #[test]
    fn never_edits_code_blocks() {
        let all = blocks();
        let code = all.iter().find(|b| b.kind == BlockKind::Code).unwrap();
        let run = validate_review_edits(&[raw(&code.id, "changed", "x")], &all, &none());
        assert!(run.validated.is_empty());
        assert_eq!(reasons(&run), [DropReason::CodeBlock]);
    }

    #[test]
    fn skips_occupied_blocks() {
        let occupied = BTreeSet::from([id_of("One.")]);
        let run = validate_review_edits(&[edit("One."), edit("Two.")], &blocks(), &occupied);
        let ids: Vec<&str> = run.validated.iter().map(|e| e.block_id.as_str()).collect();
        assert_eq!(ids, [id_of("Two.").as_str()]);
        assert_eq!(reasons(&run), [DropReason::Occupied]);
    }

    #[test]
    fn keeps_only_the_first_edit_for_a_block() {
        let run = validate_review_edits(
            &[edit_with("One.", "First"), edit_with("One.", "Second")],
            &blocks(),
            &none(),
        );
        let replacements: Vec<&str> = run
            .validated
            .iter()
            .map(|e| e.replacement.as_str())
            .collect();
        assert_eq!(replacements, ["First"]);
        assert_eq!(reasons(&run), [DropReason::DuplicateBlock]);
    }

    #[test]
    fn caps_the_validated_edits_at_6_keeping_the_first_ones() {
        let texts = [
            "One.", "Two.", "Three.", "Four.", "Five.", "Six.", "Seven.", "Eight.",
        ];
        let edits: Vec<ReviewEdit> = texts.iter().map(|t| edit(t)).collect();
        let run = validate_review_edits(&edits, &blocks(), &none());
        assert_eq!(MAX_REVIEW_EDITS, 6);
        let kept: Vec<&str> = run
            .validated
            .iter()
            .map(|e| e.block.text.as_str())
            .collect();
        assert_eq!(kept, texts[..6]);
        assert_eq!(
            reasons(&run),
            [DropReason::OverLimit, DropReason::OverLimit]
        );
    }

    #[test]
    fn does_not_count_dropped_edits_towards_the_cap() {
        let edits = [
            raw("p99", "x", "y"),
            edit_with("One.", "One."),
            edit("Two."),
        ];
        assert_eq!(
            validate_review_edits(&edits, &blocks(), &none())
                .validated
                .len(),
            1
        );
    }

    #[test]
    fn handles_an_empty_edit_list() {
        assert_eq!(
            validate_review_edits(&[], &blocks(), &none()),
            ReviewValidation::default()
        );
    }

    #[test]
    fn validates_against_the_sample_document() {
        let sample = extract_blocks(SAMPLE_DOCUMENT);
        let run = validate_review_edits(
            &[
                raw(
                    "p12",
                    "Once the canary looks healthy, you will receive a prompt to promote it to all traffic. If anything looks wrong, the rollback happens automatically and the on-call engineer is notified.",
                    "Typo.",
                ),
                raw("p11", "Nope", "Code."),
            ],
            &sample,
            &none(),
        );
        let ids: Vec<&str> = run.validated.iter().map(|e| e.block_id.as_str()).collect();
        assert_eq!(ids, ["p12"]);
    }

    const LONG: &str = "Our pipeline has changed a lot since then and every deploy now runs automated smoke tests before it reaches customers.";

    fn words(count: usize) -> String {
        (0..count)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn junk_names_its_thresholds() {
        assert_eq!(JUNK_MIN_ORIGINAL_WORDS, 15);
        assert_eq!(JUNK_MAX_REPLACEMENT_WORDS, 3);
    }

    #[test]
    fn flags_a_tiny_stub_for_a_substantial_block() {
        assert!(is_junk_replacement(LONG, "x"));
        assert!(is_junk_replacement(LONG, "Fixed it."));
        assert!(is_junk_replacement(LONG, "- "));
    }

    #[test]
    fn lets_a_legitimate_condensing_pass() {
        assert!(!is_junk_replacement(
            LONG,
            "Every deploy now runs smoke tests."
        ));
        assert!(!is_junk_replacement(
            LONG,
            "Smoke tests run before each deploy."
        ));
    }

    #[test]
    fn sits_exactly_on_the_thresholds() {
        assert!(!is_junk_replacement(&words(15), "x"));
        assert!(is_junk_replacement(&words(16), "x"));
        assert!(is_junk_replacement(&words(16), &words(2)));
        assert!(!is_junk_replacement(&words(16), &words(3)));
    }

    #[test]
    fn never_flags_a_heading_however_short_its_replacement() {
        let heading_blocks = extract_blocks(&format!("## {LONG}\n\nBody.\n"));
        assert_eq!(heading_blocks[0].kind, BlockKind::Heading);
        let run = validate_review_edits(
            &[raw("p1", "## Deploys", "Shorter.")],
            &heading_blocks,
            &none(),
        );
        assert!(run.dropped.is_empty());
        assert_eq!(run.validated[0].block_id, "p1");
    }

    #[test]
    fn does_not_apply_to_short_blocks() {
        assert!(!is_junk_replacement("Short one here.", "x"));
        assert!(!is_junk_replacement("Item one two three four.", "Done."));
    }

    #[test]
    fn flags_a_chatty_opener_the_block_did_not_have() {
        assert!(is_junk_replacement(
            "Smoke tests now run before every deploy to customers.",
            "Sure! Here's the improved version of that paragraph for you."
        ));
        assert!(!is_junk_replacement(
            "Thank you for reading our long pipeline guide today.",
            "Thank you for reading our pipeline guide today."
        ));
    }

    #[test]
    fn flags_a_replacement_that_shares_nothing_with_a_substantial_block() {
        let original = "Smoke tests now run before every single deploy reaches customers.";
        assert!(is_junk_replacement(
            original,
            "The weather tomorrow looks sunny with gentle coastal breezes."
        ));
        assert!(!is_junk_replacement(
            original,
            "Every deploy gets smoke tests first."
        ));
    }

    #[test]
    fn does_not_count_markdown_syntax_as_words() {
        assert!(is_junk_replacement(LONG, "## - * 1."));
        assert!(!is_junk_replacement(LONG, "- Smoke tests\n- Canary"));
    }

    #[test]
    fn drops_a_junk_review_edit_and_says_why() {
        let long_blocks = extract_blocks(&format!("{LONG}\n\nShort one.\n"));
        let run = validate_review_edits(&[raw("p1", "x", "Shorter.")], &long_blocks, &none());
        assert!(run.validated.is_empty());
        assert_eq!(reasons(&run), [DropReason::Junk]);
    }

    #[test]
    fn a_junk_edit_does_not_count_towards_the_cap_or_mark_its_block_as_seen() {
        let context = ReviewValidationContext {
            blocks: extract_blocks(&format!("{LONG}\n\nShort one.\n")),
            occupied: none(),
        };
        let initial = ReviewValidationState::default();
        let (state, _) = validate_review_edit(&context, &initial, &raw("p1", "x", "y"));
        assert_eq!(state, initial);
        let (_, outcome) = validate_review_edit(
            &context,
            &state,
            &raw("p1", "Every deploy now runs smoke tests.", "Shorter."),
        );
        assert!(matches!(outcome, ReviewOutcome::Validated(_)));
    }

    #[test]
    fn threads_state_through_successive_edits_without_mutating_it() {
        let context = ReviewValidationContext {
            blocks: blocks(),
            occupied: none(),
        };
        let initial = ReviewValidationState::default();
        let (first_state, first) = validate_review_edit(&context, &initial, &edit("One."));
        assert!(matches!(first, ReviewOutcome::Validated(_)));
        assert_eq!(initial.validated, 0);
        let (_, second) = validate_review_edit(&context, &first_state, &edit_with("One.", "Again"));
        assert!(matches!(
            second,
            ReviewOutcome::Dropped(DroppedEdit {
                reason: DropReason::DuplicateBlock,
                ..
            })
        ));
        assert_eq!(first_state.validated, 1);
    }

    fn output(replacement: &str, rationale: &str) -> SuggestionOutput {
        SuggestionOutput {
            replacement: replacement.into(),
            rationale: rationale.into(),
        }
    }

    #[test]
    fn check_passes_a_changed_replacement() {
        assert_eq!(
            check_suggestion("Hello world", &output("Hi", "Short.")),
            Ok(SuggestionResult {
                replacement: "Hi".into(),
                rationale: "Short.".into(),
                unchanged: false
            })
        );
    }

    #[test]
    fn check_flags_a_replacement_equal_to_the_original_as_unchanged() {
        let result = check_suggestion("Hello world", &output(" Hello world\n", "Fine.")).unwrap();
        assert!(result.unchanged);
    }

    #[test]
    fn check_rejects_an_empty_replacement() {
        assert_eq!(
            check_suggestion("Hello world", &output("  ", "x")),
            Err(InvalidSuggestion::Empty)
        );
    }
}
