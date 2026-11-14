use serde::{Deserialize, Serialize};

use super::types::{Instruction, TargetSource};
use crate::text::js_trim;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAction {
    pub id: String,
    pub label: String,
    pub instruction: String,
    #[serde(default)]
    pub skill_only: bool,
}

pub fn quick_action_instruction(action: &QuickAction) -> Instruction {
    Instruction::QuickAction {
        id: action.id.clone(),
        label: action.label.clone(),
        text: action.instruction.clone(),
    }
}

pub fn free_text_instruction(text: &str) -> Instruction {
    Instruction::FreeText {
        text: js_trim(text).to_string(),
    }
}

pub fn instruction_label(instruction: &Instruction) -> &str {
    match instruction {
        Instruction::QuickAction { label, .. } => label,
        Instruction::FreeText { text } => text,
    }
}

fn block_range(block_ids: &[&str]) -> String {
    match (block_ids.first(), block_ids.last()) {
        (Some(first), Some(last)) if first == last => (*first).to_string(),
        (Some(first), Some(last)) => format!("{first}–{last}"),
        _ => "document".to_string(),
    }
}

pub fn task_label(source: TargetSource, block_ids: &[&str], instruction: &Instruction) -> String {
    let location = match source {
        TargetSource::Selection => format!("select {}", block_range(block_ids)),
        TargetSource::Block => format!("cursor in {}", block_range(block_ids)),
    };
    let what = match instruction {
        Instruction::QuickAction { label, .. } => label.clone(),
        Instruction::FreeText { text } => format!("free text: {text}"),
    };
    format!("{location} → {what}")
}

pub fn refine_task_label(block_ids: &[&str], instruction: &Instruction) -> String {
    format!(
        "refine {} → {}",
        block_range(block_ids),
        instruction_label(instruction)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shorter() -> QuickAction {
        QuickAction {
            id: "shorter".into(),
            label: "Shorter".into(),
            instruction:
                "Make the text shorter. Remove redundancy and filler, and keep every key point."
                    .into(),
            skill_only: false,
        }
    }

    fn fix_grammar() -> QuickAction {
        QuickAction {
            id: "fix-grammar".into(),
            label: "Fix grammar".into(),
            instruction:
                "Fix grammar, spelling and punctuation errors only. Do not rephrase anything else."
                    .into(),
            skill_only: false,
        }
    }

    fn clearer() -> QuickAction {
        QuickAction {
            id: "clearer".into(),
            label: "Clearer".into(),
            instruction: "Make the text clearer and easier to follow. Keep the meaning and the level of detail.".into(),
            skill_only: false,
        }
    }

    mod instruction_builders {
        use super::*;

        #[test]
        fn builds_a_quick_action_instruction_from_the_catalog() {
            let action = shorter();
            assert_eq!(
                quick_action_instruction(&action),
                Instruction::QuickAction {
                    id: "shorter".into(),
                    label: "Shorter".into(),
                    text: action.instruction.clone(),
                }
            );
        }

        #[test]
        fn builds_a_free_text_instruction_and_trims_it() {
            assert_eq!(
                free_text_instruction("  make it punchy  "),
                Instruction::FreeText {
                    text: "make it punchy".into()
                }
            );
        }

        #[test]
        fn labels_instructions_for_display() {
            assert_eq!(
                instruction_label(&quick_action_instruction(&fix_grammar())),
                "Fix grammar"
            );
            assert_eq!(
                instruction_label(&free_text_instruction("make it punchy")),
                "make it punchy"
            );
        }
    }

    mod task_label_tests {
        use super::*;

        #[test]
        fn labels_a_selection_with_a_quick_action() {
            assert_eq!(
                task_label(
                    TargetSource::Selection,
                    &["p3"],
                    &quick_action_instruction(&shorter())
                ),
                "select p3 → Shorter"
            );
        }

        #[test]
        fn labels_a_selection_that_spans_several_blocks() {
            assert_eq!(
                task_label(
                    TargetSource::Selection,
                    &["p3", "p4", "p5"],
                    &quick_action_instruction(&clearer())
                ),
                "select p3–p5 → Clearer"
            );
        }

        #[test]
        fn labels_the_block_under_the_cursor_with_free_text() {
            assert_eq!(
                task_label(
                    TargetSource::Block,
                    &["p5"],
                    &free_text_instruction("make it friendlier")
                ),
                "cursor in p5 → free text: make it friendlier"
            );
        }
    }

    mod refine_task_label_tests {
        use super::*;

        #[test]
        fn labels_a_refine_with_the_block_and_the_follow_up() {
            assert_eq!(
                refine_task_label(&["p3"], &free_text_instruction("make it shorter")),
                "refine p3 → make it shorter"
            );
        }

        #[test]
        fn labels_a_refine_over_several_blocks() {
            assert_eq!(
                refine_task_label(&["p3", "p4"], &free_text_instruction("now more formal")),
                "refine p3–p4 → now more formal"
            );
        }
    }
}
