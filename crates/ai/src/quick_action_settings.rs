use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::quick_actions::{
    DEFAULT_QUICK_ACTION_IDS, QuickAction, get_quick_action, is_built_in_quick_action_id,
    quick_actions, quick_actions_for,
};
use crate::text::{js_trim, utf16_len};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickActionSlot {
    pub action: QuickAction,
    pub on: bool,
}

pub type QuickActionSettings = Vec<QuickActionSlot>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickActionDraft {
    pub label: String,
    pub instruction: String,
    pub skill_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuickActionDraftErrors {
    pub label: Option<String>,
    pub instruction: Option<String>,
}

pub const MAX_LABEL_LENGTH: usize = 40;
pub const MAX_INSTRUCTION_LENGTH: usize = 2000;

const SETTINGS_VERSION: i64 = 1;
const TAKEN_LABEL: &str = "Another Quick action has this name.";

static USER_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^user-[A-Za-z0-9-]{1,64}$").expect("valid regex"));

pub fn default_quick_action_settings() -> QuickActionSettings {
    let off = quick_actions()
        .into_iter()
        .filter(|action| !DEFAULT_QUICK_ACTION_IDS.contains(&action.id.as_str()))
        .map(|action| QuickActionSlot { action, on: false });
    DEFAULT_QUICK_ACTION_IDS
        .iter()
        .filter_map(|id| get_quick_action(id))
        .map(|action| QuickActionSlot { action, on: true })
        .chain(off)
        .collect()
}

pub fn is_user_quick_action_id(id: &str) -> bool {
    USER_ID.is_match(id)
}

pub fn is_user_quick_action(action: &QuickAction) -> bool {
    is_user_quick_action_id(&action.id)
}

pub fn enabled_quick_actions(settings: &[QuickActionSlot], is_skill: bool) -> Vec<QuickAction> {
    let on: Vec<QuickAction> = settings
        .iter()
        .filter(|slot| slot.on)
        .map(|slot| slot.action.clone())
        .collect();
    quick_actions_for(is_skill, &on)
}

pub fn toggle_quick_action(
    settings: &[QuickActionSlot],
    id: &str,
    on: bool,
) -> QuickActionSettings {
    settings
        .iter()
        .map(|slot| {
            if slot.action.id == id {
                QuickActionSlot { on, ..slot.clone() }
            } else {
                slot.clone()
            }
        })
        .collect()
}

pub fn move_quick_action(settings: &[QuickActionSlot], id: &str, delta: i8) -> QuickActionSettings {
    let Some(from) = settings.iter().position(|slot| slot.action.id == id) else {
        return settings.to_vec();
    };
    let Some(to) = from.checked_add_signed(isize::from(delta)) else {
        return settings.to_vec();
    };
    if to >= settings.len() {
        return settings.to_vec();
    }
    let mut moved = settings.to_vec();
    moved.swap(from, to);
    moved
}

fn user_action(id: &str, draft: &QuickActionDraft) -> QuickAction {
    QuickAction {
        id: id.to_owned(),
        label: js_trim(&draft.label).to_owned(),
        instruction: js_trim(&draft.instruction).to_owned(),
        skill_only: draft.skill_only,
    }
}

pub fn add_user_quick_action(
    settings: &[QuickActionSlot],
    draft: &QuickActionDraft,
    id: &str,
) -> QuickActionSettings {
    let mut next = settings.to_vec();
    next.push(QuickActionSlot {
        action: user_action(id, draft),
        on: true,
    });
    next
}

pub fn update_user_quick_action(
    settings: &[QuickActionSlot],
    id: &str,
    draft: &QuickActionDraft,
) -> QuickActionSettings {
    if !is_user_quick_action_id(id) || !settings.iter().any(|slot| slot.action.id == id) {
        return settings.to_vec();
    }
    settings
        .iter()
        .map(|slot| {
            if slot.action.id == id {
                QuickActionSlot {
                    action: user_action(id, draft),
                    on: slot.on,
                }
            } else {
                slot.clone()
            }
        })
        .collect()
}

pub fn remove_user_quick_action(settings: &[QuickActionSlot], id: &str) -> QuickActionSettings {
    if !is_user_quick_action_id(id) {
        return settings.to_vec();
    }
    settings
        .iter()
        .filter(|slot| slot.action.id != id)
        .cloned()
        .collect()
}

fn same_label(a: &str, b: &str) -> bool {
    js_trim(a).to_lowercase() == js_trim(b).to_lowercase()
}

fn label_error(settings: &[QuickActionSlot], label: &str, editing: Option<&str>) -> Option<String> {
    let trimmed = js_trim(label);
    if trimmed.is_empty() {
        return Some("Give it a name.".to_owned());
    }
    if utf16_len(trimmed) > MAX_LABEL_LENGTH {
        return Some(format!("Use at most {MAX_LABEL_LENGTH} characters."));
    }
    let taken = settings.iter().any(|slot| {
        Some(slot.action.id.as_str()) != editing && same_label(&slot.action.label, label)
    });
    taken.then(|| TAKEN_LABEL.to_owned())
}

fn instruction_error(instruction: &str) -> Option<String> {
    let trimmed = js_trim(instruction);
    if trimmed.is_empty() {
        return Some("Say what to do.".to_owned());
    }
    (utf16_len(trimmed) > MAX_INSTRUCTION_LENGTH)
        .then(|| format!("Use at most {MAX_INSTRUCTION_LENGTH} characters."))
}

pub fn check_quick_action_draft(
    settings: &[QuickActionSlot],
    draft: &QuickActionDraft,
    editing: Option<&str>,
) -> QuickActionDraftErrors {
    QuickActionDraftErrors {
        label: label_error(settings, &draft.label, editing),
        instruction: instruction_error(&draft.instruction),
    }
}

pub const fn draft_is_valid(errors: &QuickActionDraftErrors) -> bool {
    errors.label.is_none() && errors.instruction.is_none()
}

#[derive(Serialize)]
struct StoredSlot<'a> {
    id: &'a str,
    on: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instruction: Option<&'a str>,
    #[serde(rename = "skillOnly", skip_serializing_if = "Option::is_none")]
    skill_only: Option<bool>,
}

#[derive(Serialize)]
struct StoredSettings<'a> {
    version: i64,
    actions: Vec<StoredSlot<'a>>,
}

fn stored_slot(slot: &QuickActionSlot) -> Option<StoredSlot<'_>> {
    let id = slot.action.id.as_str();
    if is_user_quick_action_id(id) {
        Some(StoredSlot {
            id,
            on: slot.on,
            label: Some(&slot.action.label),
            instruction: Some(&slot.action.instruction),
            skill_only: Some(slot.action.skill_only),
        })
    } else if is_built_in_quick_action_id(id) {
        Some(StoredSlot {
            id,
            on: slot.on,
            label: None,
            instruction: None,
            skill_only: None,
        })
    } else {
        None
    }
}

pub fn serialize_quick_action_settings(settings: &[QuickActionSlot]) -> String {
    let stored = StoredSettings {
        version: SETTINGS_VERSION,
        actions: settings.iter().filter_map(stored_slot).collect(),
    };
    let text = serde_json::to_string_pretty(&stored).expect("settings serialize");
    format!("{text}\n")
}

fn read_slot(value: &Value, kept: &[QuickActionSlot]) -> Option<QuickActionSlot> {
    let record = value.as_object()?;
    let on = record.get("on")?.as_bool()?;
    let id = record.get("id")?.as_str()?;
    if kept.iter().any(|slot| slot.action.id == id) {
        return None;
    }
    if let Some(action) = get_quick_action(id) {
        return Some(QuickActionSlot { action, on });
    }
    if !is_user_quick_action_id(id) {
        return None;
    }
    let text = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let draft = QuickActionDraft {
        label: text("label"),
        instruction: text("instruction"),
        skill_only: record.get("skillOnly").and_then(Value::as_bool) == Some(true),
    };
    let reserved: QuickActionSettings = quick_actions()
        .into_iter()
        .map(|action| QuickActionSlot { action, on: false })
        .chain(kept.iter().cloned())
        .collect();
    let errors = check_quick_action_draft(&reserved, &draft, None);
    if draft_is_valid(&errors) {
        return Some(QuickActionSlot {
            action: user_action(id, &draft),
            on,
        });
    }
    if errors.label.as_deref() != Some(TAKEN_LABEL) {
        return None;
    }
    let renamed = QuickActionDraft {
        label: format!("{} (mine)", js_trim(&draft.label)),
        ..draft
    };
    draft_is_valid(&check_quick_action_draft(&reserved, &renamed, None)).then(|| QuickActionSlot {
        action: user_action(id, &renamed),
        on,
    })
}

fn with_missing_built_ins(settings: QuickActionSettings) -> QuickActionSettings {
    let missing: Vec<QuickActionSlot> = quick_actions()
        .into_iter()
        .filter(|action| !settings.iter().any(|slot| slot.action.id == action.id))
        .map(|action| QuickActionSlot { action, on: false })
        .collect();
    settings.into_iter().chain(missing).collect()
}

pub fn parse_quick_action_settings(text: &str) -> Option<QuickActionSettings> {
    let parsed: Value = serde_json::from_str(text).ok()?;
    let record = parsed.as_object()?;
    if record.get("version")?.as_i64() != Some(SETTINGS_VERSION) {
        return None;
    }
    let actions = record.get("actions")?.as_array()?;
    let slots = actions.iter().fold(Vec::new(), |mut kept, value| {
        if let Some(slot) = read_slot(value, &kept) {
            kept.push(slot);
        }
        kept
    });
    Some(with_missing_built_ins(slots))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ids(settings: &[QuickActionSlot]) -> Vec<String> {
        settings.iter().map(|slot| slot.action.id.clone()).collect()
    }

    fn on_ids(settings: &[QuickActionSlot]) -> Vec<String> {
        enabled_quick_actions(settings, true)
            .into_iter()
            .map(|action| action.id)
            .collect()
    }

    fn draft() -> QuickActionDraft {
        QuickActionDraft {
            label: "Warmer".into(),
            instruction: "Make the tone warmer.".into(),
            skill_only: false,
        }
    }

    fn with_warmer() -> QuickActionSettings {
        add_user_quick_action(&default_quick_action_settings(), &draft(), "user-1")
    }

    fn strs(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn default_settings_list_every_built_in_with_three_on() {
        let defaults = default_quick_action_settings();
        assert_eq!(
            ids(&defaults),
            strs(&[
                "shorter",
                "clearer",
                "fix-grammar",
                "more-formal",
                "make-instruction",
                "cut-known"
            ])
        );
        assert_eq!(
            on_ids(&defaults),
            strs(&["shorter", "clearer", "fix-grammar"])
        );
    }

    #[test]
    fn enabled_keeps_the_order_and_drops_skill_actions_outside_a_skill() {
        let settings = toggle_quick_action(
            &toggle_quick_action(&default_quick_action_settings(), "cut-known", true),
            "shorter",
            false,
        );
        assert_eq!(
            on_ids(&settings),
            strs(&["clearer", "fix-grammar", "cut-known"])
        );
        let outside: Vec<String> = enabled_quick_actions(&settings, false)
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(outside, strs(&["clearer", "fix-grammar"]));
    }

    #[test]
    fn move_shifts_an_action_one_place() {
        let defaults = default_quick_action_settings();
        assert_eq!(
            ids(&move_quick_action(&defaults, "clearer", -1)),
            strs(&[
                "clearer",
                "shorter",
                "fix-grammar",
                "more-formal",
                "make-instruction",
                "cut-known"
            ])
        );
        assert_eq!(
            ids(&move_quick_action(&defaults, "clearer", 1))[..3],
            strs(&["shorter", "fix-grammar", "clearer"])[..]
        );
    }

    #[test]
    fn move_leaves_the_list_alone_at_either_end() {
        let defaults = default_quick_action_settings();
        assert_eq!(move_quick_action(&defaults, "shorter", -1), defaults);
        assert_eq!(move_quick_action(&defaults, "cut-known", 1), defaults);
    }

    #[test]
    fn adds_a_new_action_at_the_end_turned_on_with_trimmed_text() {
        let settings = add_user_quick_action(
            &default_quick_action_settings(),
            &QuickActionDraft {
                label: "  Warmer ".into(),
                instruction: " Make it warm. ".into(),
                skill_only: true,
            },
            "user-1",
        );
        assert_eq!(
            settings.last(),
            Some(&QuickActionSlot {
                on: true,
                action: QuickAction {
                    id: "user-1".into(),
                    label: "Warmer".into(),
                    instruction: "Make it warm.".into(),
                    skill_only: true,
                }
            })
        );
    }

    #[test]
    fn edits_a_user_action_in_place_and_keeps_whether_it_is_on() {
        let off = toggle_quick_action(&with_warmer(), "user-1", false);
        let edited = update_user_quick_action(
            &off,
            "user-1",
            &QuickActionDraft {
                label: "Kinder".into(),
                instruction: "Be kind.".into(),
                skill_only: false,
            },
        );
        assert_eq!(
            edited.last(),
            Some(&QuickActionSlot {
                on: false,
                action: QuickAction {
                    id: "user-1".into(),
                    label: "Kinder".into(),
                    instruction: "Be kind.".into(),
                    skill_only: false,
                }
            })
        );
    }

    #[test]
    fn never_edits_or_deletes_a_built_in_action() {
        let settings = with_warmer();
        assert_eq!(
            update_user_quick_action(&settings, "user-2", &draft()),
            settings
        );
        assert_eq!(
            update_user_quick_action(&settings, "shorter", &draft()),
            settings
        );
        assert_eq!(
            ids(&remove_user_quick_action(&settings, "user-1")),
            ids(&default_quick_action_settings())
        );
        assert_eq!(remove_user_quick_action(&settings, "shorter"), settings);
    }

    #[test]
    fn check_accepts_a_label_and_an_instruction() {
        let errors = check_quick_action_draft(
            &with_warmer(),
            &QuickActionDraft {
                label: "Bolder".into(),
                ..draft()
            },
            None,
        );
        assert_eq!(errors, QuickActionDraftErrors::default());
    }

    #[test]
    fn check_asks_for_a_label_and_an_instruction() {
        let errors = check_quick_action_draft(
            &with_warmer(),
            &QuickActionDraft {
                label: " ".into(),
                instruction: String::new(),
                skill_only: false,
            },
            None,
        );
        assert_eq!(errors.label.as_deref(), Some("Give it a name."));
        assert_eq!(errors.instruction.as_deref(), Some("Say what to do."));
    }

    #[test]
    fn check_refuses_a_name_another_action_has_ignoring_case() {
        for label in ["shorter", "WARMER"] {
            let errors = check_quick_action_draft(
                &with_warmer(),
                &QuickActionDraft {
                    label: label.into(),
                    ..draft()
                },
                None,
            );
            assert_eq!(
                errors,
                QuickActionDraftErrors {
                    label: Some("Another Quick action has this name.".into()),
                    instruction: None
                }
            );
        }
    }

    #[test]
    fn check_lets_an_action_keep_its_own_name_while_edited() {
        assert_eq!(
            check_quick_action_draft(&with_warmer(), &draft(), Some("user-1")),
            QuickActionDraftErrors::default()
        );
    }

    #[test]
    fn check_caps_the_label_and_the_instruction() {
        let errors = check_quick_action_draft(
            &with_warmer(),
            &QuickActionDraft {
                label: "x".repeat(41),
                instruction: "y".repeat(2001),
                skill_only: false,
            },
            None,
        );
        assert_eq!(errors.label.as_deref(), Some("Use at most 40 characters."));
        assert_eq!(
            errors.instruction.as_deref(),
            Some("Use at most 2000 characters.")
        );
    }

    #[test]
    fn stored_settings_round_trip_storing_only_the_id_of_a_built_in() {
        let settings = with_warmer();
        let text = serialize_quick_action_settings(&settings);
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["actions"][0], json!({"id": "shorter", "on": true}));
        assert!(text.ends_with("}\n"));
        assert_eq!(parse_quick_action_settings(&text), Some(settings));
    }

    #[test]
    fn built_in_wording_comes_from_the_app_not_the_file() {
        let text = json!({
            "version": 1,
            "actions": [{"id": "shorter", "on": false, "label": "Hacked", "instruction": "x"}]
        })
        .to_string();
        let parsed = parse_quick_action_settings(&text).unwrap();
        assert_eq!(
            parsed[0],
            QuickActionSlot {
                on: false,
                action: quick_actions()[0].clone()
            }
        );
    }

    #[test]
    fn adds_missing_built_ins_off_and_drops_unknown_or_repeated_ones() {
        let text = json!({
            "version": 1,
            "actions": [
                {"id": "clearer", "on": true},
                {"id": "clearer", "on": false},
                {"id": "retired", "on": true},
                {"id": "user-9", "on": true, "label": "", "instruction": "x"}
            ]
        })
        .to_string();
        let parsed = parse_quick_action_settings(&text).unwrap();
        assert_eq!(
            ids(&parsed),
            strs(&[
                "clearer",
                "shorter",
                "more-formal",
                "fix-grammar",
                "make-instruction",
                "cut-known"
            ])
        );
        assert_eq!(on_ids(&parsed), strs(&["clearer"]));
    }

    #[test]
    fn renames_a_user_action_whose_name_a_built_in_now_has() {
        let text = json!({
            "version": 1,
            "actions": [
                {"id": "user-1", "on": true, "label": "Shorter", "instruction": "x"},
                {"id": "user-2", "on": true, "label": "shorter", "instruction": "y"}
            ]
        })
        .to_string();
        let parsed = parse_quick_action_settings(&text).unwrap();
        let labels: Vec<String> = parsed
            .iter()
            .filter(|slot| slot.action.id.starts_with("user-"))
            .map(|slot| slot.action.label.clone())
            .collect();
        assert_eq!(labels, strs(&["Shorter (mine)"]));
    }

    #[test]
    fn reads_nothing_from_text_that_is_not_version_1_settings() {
        assert_eq!(parse_quick_action_settings("not json"), None);
        assert_eq!(
            parse_quick_action_settings(r#"{"version":2,"actions":[]}"#),
            None
        );
        assert_eq!(parse_quick_action_settings(r#"{"version":1}"#), None);
    }
}
