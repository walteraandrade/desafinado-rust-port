use super::frontmatter_fields::{
    FrontmatterProblem, SkillFieldValues, TextEdit, apply_edits, plan_skill_field_edits,
};

pub type TextChange = TextEdit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldEditPlan {
    Change { changes: Vec<TextChange> },
    Unchanged,
    Refused { problem: FrontmatterProblem },
}

pub fn apply_changes(text: &str, changes: &[TextChange]) -> String {
    apply_edits(text, changes)
}

pub fn plan_field_edit(text: &str, values: &SkillFieldValues) -> FieldEditPlan {
    match plan_skill_field_edits(text, values) {
        Err(problem) => FieldEditPlan::Refused { problem },
        Ok(edits) if edits.is_empty() => FieldEditPlan::Unchanged,
        Ok(edits) => FieldEditPlan::Change { changes: edits },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::frontmatter_block::find_frontmatter;
    use crate::skills::frontmatter_fields::{
        FrontmatterFields, FrontmatterProblemRule, FrontmatterReading, read_frontmatter,
    };
    use serde_json::{Value, json};

    const BODY: &str = "\n# Demo\n\nBody has name: nope and a rule.\n\n---\n\nAfter.\n";

    fn after_closing() -> String {
        format!("\n{BODY}")
    }

    fn skill() -> String {
        format!("---\nname: demo\ndescription: Does a thing.\nlicense: MIT\n---\n{BODY}")
    }

    fn changes_of(plan: FieldEditPlan) -> Vec<TextChange> {
        match plan {
            FieldEditPlan::Change { changes } => changes,
            other => panic!("{other:?}"),
        }
    }

    fn edited(text: &str, values: &SkillFieldValues) -> String {
        apply_changes(text, &changes_of(plan_field_edit(text, values)))
    }

    fn fields_of(text: &str) -> FrontmatterFields {
        match read_frontmatter(text) {
            FrontmatterReading::Present(fields) => fields,
            other => panic!("{other:?}"),
        }
    }

    fn after_frontmatter(text: &str) -> &str {
        &text[find_frontmatter(text).map_or(0, |found| found.to)..]
    }

    fn change(from: usize, to: usize, insert: &str) -> TextChange {
        TextChange {
            from,
            to,
            insert: insert.to_string(),
        }
    }

    #[test]
    fn apply_changes_applies_changes_given_in_the_coordinates_of_the_original_text() {
        assert_eq!(
            apply_changes("abcdef", &[change(4, 5, "X"), change(1, 2, "YY")]),
            "aYYcdXf"
        );
    }

    #[test]
    fn apply_changes_returns_the_text_when_there_is_nothing_to_apply() {
        assert_eq!(apply_changes("abc", &[]), "abc");
    }

    #[test]
    fn plan_changes_only_the_value_of_the_field() {
        let text = skill();
        let changes = changes_of(plan_field_edit(&text, &SkillFieldValues::name("renamed")));
        assert_eq!(changes.len(), 1);
        assert_eq!(
            apply_changes(&text, &changes),
            text.replace("name: demo", "name: renamed")
        );
        let replaced = &text[changes[0].from..changes[0].to];
        assert!(replaced.contains("demo"));
        assert!(!replaced.contains("description"));
    }

    #[test]
    fn plan_leaves_the_other_keys_their_order_and_the_body_byte_identical() {
        let text = skill();
        let next = edited(&text, &SkillFieldValues::description("Does another thing."));
        assert_eq!(next, text.replace("Does a thing.", "Does another thing."));
        assert_eq!(after_frontmatter(&next), after_frontmatter(&text));
    }

    #[test]
    fn plan_keeps_every_change_inside_the_frontmatter() {
        let text = skill();
        let frontmatter = find_frontmatter(&text).expect("frontmatter");
        let closing = text[frontmatter.yaml_from..]
            .find("\n---")
            .map(|index| index + frontmatter.yaml_from + 1)
            .expect("closing");
        for edit in changes_of(plan_field_edit(&text, &SkillFieldValues::both("a", "b"))) {
            assert!(edit.from >= frontmatter.yaml_from);
            assert!(edit.to <= closing);
        }
    }

    #[test]
    fn plan_edits_both_fields_in_one_plan_in_the_coordinates_of_the_original() {
        let text = skill();
        let changes = changes_of(plan_field_edit(&text, &SkillFieldValues::both("x", "Y.")));
        assert_eq!(changes.len(), 2);
        assert!(changes[0].to <= changes[1].from);
        let next = apply_changes(&text, &changes);
        let fields = fields_of(&next);
        assert_eq!(fields["name"], json!("x"));
        assert_eq!(fields["description"], json!("Y."));
        assert_eq!(after_frontmatter(&next), after_closing());
    }

    #[test]
    fn plan_keeps_the_yaml_valid_for_values_that_need_quoting() {
        for value in [
            "a: b",
            "# not a comment",
            "- item",
            "\"quoted\"",
            "true",
            "1",
        ] {
            let next = edited(&skill(), &SkillFieldValues::description(value));
            assert_eq!(fields_of(&next)["description"], json!(value));
            assert_eq!(fields_of(&next)["name"], json!("demo"));
        }
    }

    #[test]
    fn plan_keeps_the_yaml_valid_for_a_multi_line_description() {
        let value = "First line.\nSecond line.";
        let next = edited(&skill(), &SkillFieldValues::description(value));
        assert_eq!(fields_of(&next)["description"], json!(value));
        assert_eq!(after_frontmatter(&next), after_closing());
    }

    #[test]
    fn plan_keeps_the_yaml_valid_for_an_empty_value() {
        let next = edited(&skill(), &SkillFieldValues::name(""));
        assert_eq!(fields_of(&next)["name"], json!(""));
        assert_eq!(fields_of(&next)["license"], json!("MIT"));
    }

    #[test]
    fn plan_is_unchanged_when_the_value_is_the_same() {
        assert_eq!(
            plan_field_edit(&skill(), &SkillFieldValues::name("demo")),
            FieldEditPlan::Unchanged
        );
        assert_eq!(
            plan_field_edit(&skill(), &SkillFieldValues::default()),
            FieldEditPlan::Unchanged
        );
    }

    #[test]
    fn no_frontmatter_adds_it_with_one_insertion_at_the_start() {
        let plain = "# Title\n\nText.\n";
        assert_eq!(
            changes_of(plan_field_edit(plain, &SkillFieldValues::name("demo"))),
            vec![change(0, 0, "---\nname: demo\n---\n\n")]
        );
        assert_eq!(
            edited(plain, &SkillFieldValues::name("demo")),
            "---\nname: demo\n---\n\n# Title\n\nText.\n"
        );
    }

    #[test]
    fn no_frontmatter_does_not_reuse_the_first_character_of_a_body_that_starts_with_a_dash() {
        let body = "- item\n";
        assert_eq!(
            changes_of(plan_field_edit(body, &SkillFieldValues::name("x"))),
            vec![change(0, 0, "---\nname: x\n---\n\n")]
        );
        assert_eq!(
            edited(body, &SkillFieldValues::name("x")),
            "---\nname: x\n---\n\n- item\n"
        );
    }

    #[test]
    fn no_frontmatter_does_not_reuse_the_start_of_a_body_that_opens_with_three_dashes_or_a_key() {
        for body in ["---x\n", "name: y\n", "> quote\n"] {
            let next = edited(body, &SkillFieldValues::name("x"));
            assert_eq!(next, format!("---\nname: x\n---\n\n{body}"));
            assert_eq!(Value::Object(fields_of(&next)), json!({"name": "x"}));
        }
    }

    #[test]
    fn plan_adds_a_missing_field_next_to_the_others_with_a_pure_insertion() {
        let text = format!("---\nname: demo\n---\n{BODY}");
        let changes = changes_of(plan_field_edit(
            &text,
            &SkillFieldValues::description("Does it."),
        ));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].from, changes[0].to);
        let next = apply_changes(&text, &changes);
        assert_eq!(
            Value::Object(fields_of(&next)),
            json!({"name": "demo", "description": "Does it."})
        );
        assert_eq!(after_frontmatter(&next), after_closing());
    }

    #[test]
    fn plan_refuses_a_document_with_a_frontmatter_problem() {
        for text in [
            "---\nname: demo\n\n# Title\n",
            "---\nthis is prose\n---\n\nText.\n",
            "---\nname: [unclosed\n---\n\nText.\n",
        ] {
            assert!(matches!(
                plan_field_edit(text, &SkillFieldValues::name("demo")),
                FieldEditPlan::Refused { .. }
            ));
        }
    }

    #[test]
    fn plan_names_the_reason_it_refused() {
        assert_eq!(
            plan_field_edit("---\nname: demo\n\n# Title\n", &SkillFieldValues::name("x")),
            FieldEditPlan::Refused {
                problem: FrontmatterProblem {
                    rule: FrontmatterProblemRule::FrontmatterNotClosed,
                    message: "frontmatter not closed".to_string(),
                    line: None,
                    column: None,
                }
            }
        );
    }
}
