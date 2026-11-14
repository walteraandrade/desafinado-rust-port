use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAction {
    pub id: String,
    pub label: String,
    pub instruction: String,
    #[serde(default)]
    pub skill_only: bool,
}

struct BuiltIn {
    id: &'static str,
    label: &'static str,
    instruction: &'static str,
    skill_only: bool,
}

const BUILT_INS: [BuiltIn; 6] = [
    BuiltIn {
        id: "shorter",
        label: "Shorter",
        instruction: "Make the text shorter. Remove redundancy and filler, and keep every key point.",
        skill_only: false,
    },
    BuiltIn {
        id: "clearer",
        label: "Clearer",
        instruction: "Make the text clearer and easier to follow. Keep the meaning and the level of detail.",
        skill_only: false,
    },
    BuiltIn {
        id: "more-formal",
        label: "More formal",
        instruction: "Make the tone more formal and professional. Keep the meaning and the level of detail.",
        skill_only: false,
    },
    BuiltIn {
        id: "fix-grammar",
        label: "Fix grammar",
        instruction: "Fix grammar, spelling and punctuation errors only. Do not rephrase anything else.",
        skill_only: false,
    },
    BuiltIn {
        id: "make-instruction",
        label: "Make this an instruction",
        instruction: "Rewrite the text as a direct instruction to the agent, in the imperative mood. Keep every requirement and do not add new ones.",
        skill_only: true,
    },
    BuiltIn {
        id: "cut-known",
        label: "Cut what the agent already knows",
        instruction: "Remove explanations of things a capable coding agent already knows, such as general programming concepts and tool basics. Keep what is specific to this task, this project or this team.",
        skill_only: true,
    },
];

pub const DEFAULT_QUICK_ACTION_IDS: [&str; 3] = ["shorter", "clearer", "fix-grammar"];

fn from_built_in(built_in: &BuiltIn) -> QuickAction {
    QuickAction {
        id: built_in.id.to_owned(),
        label: built_in.label.to_owned(),
        instruction: built_in.instruction.to_owned(),
        skill_only: built_in.skill_only,
    }
}

pub fn quick_actions() -> Vec<QuickAction> {
    BUILT_INS.iter().map(from_built_in).collect()
}

pub fn is_built_in_quick_action_id(id: &str) -> bool {
    BUILT_INS.iter().any(|candidate| candidate.id == id)
}

pub fn get_quick_action(id: &str) -> Option<QuickAction> {
    BUILT_INS
        .iter()
        .find(|candidate| candidate.id == id)
        .map(from_built_in)
}

pub fn default_quick_actions() -> Vec<QuickAction> {
    DEFAULT_QUICK_ACTION_IDS
        .iter()
        .filter_map(|id| get_quick_action(id))
        .collect()
}

pub fn quick_actions_for(is_skill: bool, actions: &[QuickAction]) -> Vec<QuickAction> {
    actions
        .iter()
        .filter(|action| is_skill || !action.skill_only)
        .cloned()
        .collect()
}
