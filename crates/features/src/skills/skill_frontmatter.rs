use icu_normalizer::ComposingNormalizerBorrowed;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::LazyLock;

use crate::text::{char_count, js_trim};

pub const AGENT_SKILLS_SPEC_URL: &str = "https://agentskills.io/specification";
pub const NAME_MAX_LENGTH: usize = 64;
pub const DESCRIPTION_MAX_LENGTH: usize = 1024;
pub const COMPATIBILITY_MAX_LENGTH: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillRule {
    NameMissing,
    NameNotText,
    NameEmpty,
    NameTooLong,
    NameNotLowercase,
    NameInvalidCharacters,
    NameEdgeHyphen,
    NameDoubleHyphen,
    NameFolderMismatch,
    DescriptionMissing,
    DescriptionNotText,
    DescriptionEmpty,
    DescriptionTooLong,
    CompatibilityNotText,
    CompatibilityEmpty,
    CompatibilityTooLong,
}

impl SkillRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NameMissing => "name-missing",
            Self::NameNotText => "name-not-text",
            Self::NameEmpty => "name-empty",
            Self::NameTooLong => "name-too-long",
            Self::NameNotLowercase => "name-not-lowercase",
            Self::NameInvalidCharacters => "name-invalid-characters",
            Self::NameEdgeHyphen => "name-edge-hyphen",
            Self::NameDoubleHyphen => "name-double-hyphen",
            Self::NameFolderMismatch => "name-folder-mismatch",
            Self::DescriptionMissing => "description-missing",
            Self::DescriptionNotText => "description-not-text",
            Self::DescriptionEmpty => "description-empty",
            Self::DescriptionTooLong => "description-too-long",
            Self::CompatibilityNotText => "compatibility-not-text",
            Self::CompatibilityEmpty => "compatibility-empty",
            Self::CompatibilityTooLong => "compatibility-too-long",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckedField {
    Name,
    Description,
    Compatibility,
}

impl CheckedField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Description => "description",
            Self::Compatibility => "compatibility",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillProblem {
    pub field: CheckedField,
    pub rule: SkillRule,
    pub message: String,
}

static NOT_LETTER_DIGIT_OR_HYPHEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}-]").expect("valid pattern"));

fn problem(field: CheckedField, rule: SkillRule, message: impl Into<String>) -> SkillProblem {
    SkillProblem {
        field,
        rule,
        message: message.into(),
    }
}

fn nfkc(text: &str) -> String {
    ComposingNormalizerBorrowed::new_nfkc()
        .normalize(text)
        .into_owned()
}

fn too_long(field: CheckedField, length: usize, limit: usize) -> SkillProblem {
    problem(
        field,
        match field {
            CheckedField::Name => SkillRule::NameTooLong,
            CheckedField::Description => SkillRule::DescriptionTooLong,
            CheckedField::Compatibility => SkillRule::CompatibilityTooLong,
        },
        format!(
            "{} has {length} characters; the limit is {limit}",
            field.as_str()
        ),
    )
}

enum Text<'a> {
    Value(&'a str),
    Problem(SkillProblem),
}

fn read_text(field: CheckedField, value: Option<&Value>) -> Text<'_> {
    let name = field.as_str();
    let (missing, not_text, empty) = match field {
        CheckedField::Description => (
            SkillRule::DescriptionMissing,
            SkillRule::DescriptionNotText,
            SkillRule::DescriptionEmpty,
        ),
        _ => (
            SkillRule::NameMissing,
            SkillRule::NameNotText,
            SkillRule::NameEmpty,
        ),
    };
    match value {
        None | Some(Value::Null) => {
            Text::Problem(problem(field, missing, format!("{name} is required")))
        }
        Some(Value::String(text)) if js_trim(text).is_empty() => {
            Text::Problem(problem(field, empty, format!("{name} must not be empty")))
        }
        Some(Value::String(text)) => Text::Value(text),
        Some(_) => Text::Problem(problem(field, not_text, format!("{name} must be text"))),
    }
}

fn name_problems(name: &str, folder_name: Option<&str>) -> Vec<SkillProblem> {
    let normalized = nfkc(js_trim(name));
    let length = char_count(&normalized);
    let mut problems = Vec::new();
    if length > NAME_MAX_LENGTH {
        problems.push(too_long(CheckedField::Name, length, NAME_MAX_LENGTH));
    }
    if normalized != normalized.to_lowercase() {
        problems.push(problem(
            CheckedField::Name,
            SkillRule::NameNotLowercase,
            "name must be lowercase",
        ));
    }
    if normalized.starts_with('-') || normalized.ends_with('-') {
        problems.push(problem(
            CheckedField::Name,
            SkillRule::NameEdgeHyphen,
            "name must not start or end with a hyphen",
        ));
    }
    if normalized.contains("--") {
        problems.push(problem(
            CheckedField::Name,
            SkillRule::NameDoubleHyphen,
            "name must not contain consecutive hyphens",
        ));
    }
    if NOT_LETTER_DIGIT_OR_HYPHEN.is_match(&normalized) {
        problems.push(problem(
            CheckedField::Name,
            SkillRule::NameInvalidCharacters,
            "name may only contain letters, digits and hyphens",
        ));
    }
    if let Some(folder) = folder_name
        && nfkc(folder) != normalized
    {
        problems.push(problem(
            CheckedField::Name,
            SkillRule::NameFolderMismatch,
            format!("name must match its folder name, \"{folder}\""),
        ));
    }
    problems
}

fn description_problems(description: &str) -> Vec<SkillProblem> {
    let length = char_count(description);
    if length > DESCRIPTION_MAX_LENGTH {
        vec![too_long(
            CheckedField::Description,
            length,
            DESCRIPTION_MAX_LENGTH,
        )]
    } else {
        Vec::new()
    }
}

fn compatibility_problems(value: Option<&Value>) -> Vec<SkillProblem> {
    let field = CheckedField::Compatibility;
    match value {
        None => Vec::new(),
        Some(Value::Null) => vec![problem(
            field,
            SkillRule::CompatibilityEmpty,
            "compatibility must not be empty",
        )],
        Some(Value::String(text)) if js_trim(text).is_empty() => vec![problem(
            field,
            SkillRule::CompatibilityEmpty,
            "compatibility must not be empty",
        )],
        Some(Value::String(text)) => {
            let length = char_count(text);
            if length > COMPATIBILITY_MAX_LENGTH {
                vec![too_long(field, length, COMPATIBILITY_MAX_LENGTH)]
            } else {
                Vec::new()
            }
        }
        Some(_) => vec![problem(
            field,
            SkillRule::CompatibilityNotText,
            "compatibility must be text",
        )],
    }
}

pub fn check_skill_name(name: &str, folder_name: Option<&str>) -> Vec<SkillProblem> {
    let value = Value::String(name.to_string());
    match read_text(CheckedField::Name, Some(&value)) {
        Text::Value(text) => name_problems(text, folder_name),
        Text::Problem(failure) => vec![failure],
    }
}

pub fn check_skill_frontmatter(
    fields: &Map<String, Value>,
    folder_name: Option<&str>,
) -> Vec<SkillProblem> {
    let mut problems = match read_text(CheckedField::Name, fields.get("name")) {
        Text::Value(text) => name_problems(text, folder_name),
        Text::Problem(failure) => vec![failure],
    };
    problems.extend(
        match read_text(CheckedField::Description, fields.get("description")) {
            Text::Value(text) => description_problems(text),
            Text::Problem(failure) => vec![failure],
        },
    );
    problems.extend(compatibility_problems(fields.get("compatibility")));
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("{other:?}"),
        }
    }

    fn valid() -> Map<String, Value> {
        fields(json!({"name": "pdf-processing", "description": "Extracts text from PDFs."}))
    }

    fn rules_of(value: Value, folder_name: Option<&str>) -> Vec<SkillRule> {
        check_skill_frontmatter(&fields(value), folder_name)
            .into_iter()
            .map(|problem| problem.rule)
            .collect()
    }

    fn with(base: Value, extra: Value) -> Value {
        let mut merged = fields(base);
        merged.extend(fields(extra));
        Value::Object(merged)
    }

    #[test]
    fn limits_come_from_the_agent_skills_specification() {
        assert_eq!(
            AGENT_SKILLS_SPEC_URL,
            "https://agentskills.io/specification"
        );
        assert_eq!(NAME_MAX_LENGTH, 64);
        assert_eq!(DESCRIPTION_MAX_LENGTH, 1024);
    }

    #[test]
    fn has_no_problems_for_a_valid_name_and_description() {
        assert_eq!(check_skill_frontmatter(&valid(), None), vec![]);
        assert_eq!(
            check_skill_frontmatter(&valid(), Some("pdf-processing")),
            vec![]
        );
    }

    #[test]
    fn ignores_fields_it_has_no_rule_for() {
        let extra = with(
            Value::Object(valid()),
            json!({"license": 42, "metadata": {"version": 1}, "allowed-tools": ["Read"]}),
        );
        assert_eq!(check_skill_frontmatter(&fields(extra), None), vec![]);
    }

    #[test]
    fn reports_each_problem_with_its_field_rule_and_a_message() {
        assert_eq!(
            check_skill_frontmatter(&Map::new(), None),
            vec![
                SkillProblem {
                    field: CheckedField::Name,
                    rule: SkillRule::NameMissing,
                    message: "name is required".to_string()
                },
                SkillProblem {
                    field: CheckedField::Description,
                    rule: SkillRule::DescriptionMissing,
                    message: "description is required".to_string()
                },
            ]
        );
    }

    #[test]
    fn lists_name_problems_before_description_problems() {
        assert_eq!(
            rules_of(json!({"name": "Bad Name", "description": ""}), None),
            vec![
                SkillRule::NameNotLowercase,
                SkillRule::NameInvalidCharacters,
                SkillRule::DescriptionEmpty
            ]
        );
    }

    #[test]
    fn description_is_missing_when_absent_or_null() {
        assert_eq!(
            rules_of(json!({"name": "a"}), None),
            vec![SkillRule::DescriptionMissing]
        );
        assert_eq!(
            rules_of(json!({"name": "a", "description": null}), None),
            vec![SkillRule::DescriptionMissing]
        );
    }

    #[test]
    fn description_is_empty_when_blank() {
        assert_eq!(
            rules_of(json!({"name": "a", "description": ""}), None),
            vec![SkillRule::DescriptionEmpty]
        );
        assert_eq!(
            rules_of(json!({"name": "a", "description": " \n\t "}), None),
            vec![SkillRule::DescriptionEmpty]
        );
    }

    #[test]
    fn description_must_be_text() {
        for description in [json!(42), json!(true), json!(["a"]), json!({"text": "a"})] {
            assert_eq!(
                rules_of(json!({"name": "a", "description": description}), None),
                vec![SkillRule::DescriptionNotText]
            );
        }
    }

    #[test]
    fn description_allows_up_to_1024_characters() {
        assert_eq!(
            rules_of(
                json!({"name": "a", "description": "x".repeat(DESCRIPTION_MAX_LENGTH)}),
                None
            ),
            vec![]
        );
    }

    #[test]
    fn description_is_too_long_past_1024_characters_and_says_how_long() {
        let problems = check_skill_frontmatter(
            &fields(json!({"name": "a", "description": "x".repeat(DESCRIPTION_MAX_LENGTH + 1)})),
            None,
        );
        assert_eq!(
            problems,
            vec![SkillProblem {
                field: CheckedField::Description,
                rule: SkillRule::DescriptionTooLong,
                message: "description has 1025 characters; the limit is 1024".to_string()
            }]
        );
    }

    #[test]
    fn description_counts_characters_not_utf16_units() {
        assert_eq!(
            rules_of(
                json!({"name": "a", "description": "😀".repeat(DESCRIPTION_MAX_LENGTH)}),
                None
            ),
            vec![]
        );
        assert_eq!(
            rules_of(
                json!({"name": "a", "description": "😀".repeat(DESCRIPTION_MAX_LENGTH + 1)}),
                None
            ),
            vec![SkillRule::DescriptionTooLong]
        );
    }

    fn name_rules(name: Value, folder: Option<&str>) -> Vec<SkillRule> {
        rules_of(
            json!({"name": name, "description": "Does a thing."}),
            folder,
        )
    }

    #[test]
    fn name_is_missing_when_absent_or_null() {
        assert_eq!(
            rules_of(json!({"description": "Does a thing."}), None),
            vec![SkillRule::NameMissing]
        );
        assert_eq!(name_rules(Value::Null, None), vec![SkillRule::NameMissing]);
    }

    #[test]
    fn name_is_empty_when_blank_and_gets_no_further_problems() {
        assert_eq!(name_rules(json!(""), None), vec![SkillRule::NameEmpty]);
        assert_eq!(
            name_rules(json!("  "), Some("folder")),
            vec![SkillRule::NameEmpty]
        );
    }

    #[test]
    fn name_must_be_text() {
        for name in [json!(42), json!(true), json!(["a"]), json!({"text": "a"})] {
            assert_eq!(
                name_rules(name, Some("folder")),
                vec![SkillRule::NameNotText]
            );
        }
    }

    #[test]
    fn name_allows_lowercase_letters_digits_and_single_hyphens() {
        for name in ["a", "pdf-processing", "v2-tools", "2024", "a-b-c"] {
            assert_eq!(name_rules(json!(name), None), vec![]);
        }
    }

    #[test]
    fn name_allows_lowercase_letters_outside_a_z() {
        for name in ["análise-de-dados", "ação", "データ"] {
            assert_eq!(name_rules(json!(name), None), vec![]);
        }
    }

    #[test]
    fn name_allows_up_to_64_characters() {
        assert_eq!(name_rules(json!("a".repeat(NAME_MAX_LENGTH)), None), vec![]);
    }

    #[test]
    fn name_is_too_long_past_64_characters_and_says_how_long() {
        let problems = check_skill_frontmatter(
            &fields(
                json!({"name": "a".repeat(NAME_MAX_LENGTH + 1), "description": "Does a thing."}),
            ),
            None,
        );
        assert_eq!(
            problems,
            vec![SkillProblem {
                field: CheckedField::Name,
                rule: SkillRule::NameTooLong,
                message: "name has 65 characters; the limit is 64".to_string()
            }]
        );
    }

    #[test]
    fn name_length_is_measured_after_unicode_compatibility_normalization() {
        assert_eq!(
            name_rules(json!("ａ".repeat(NAME_MAX_LENGTH)), None),
            vec![]
        );
        assert_eq!(
            name_rules(json!("ａ".repeat(NAME_MAX_LENGTH + 1)), None),
            vec![SkillRule::NameTooLong]
        );
    }

    #[test]
    fn name_must_be_lowercase() {
        for name in ["PDF-Processing", "Pdf", "ÁGUA"] {
            assert_eq!(
                name_rules(json!(name), None),
                vec![SkillRule::NameNotLowercase]
            );
        }
    }

    #[test]
    fn name_rejects_characters_other_than_letters_digits_and_hyphens() {
        for name in [
            "pdf processing",
            "pdf_processing",
            "pdf.processing",
            "pdf/processing",
            "pdf:processing",
            "pdf🙂",
        ] {
            assert_eq!(
                name_rules(json!(name), None),
                vec![SkillRule::NameInvalidCharacters],
                "{name}"
            );
        }
    }

    #[test]
    fn name_is_trimmed_before_checking_as_the_reference_validator_does() {
        for name in [" pdf", "pdf ", "pdf\n", "\t pdf-processing \n"] {
            assert_eq!(name_rules(json!(name), None), vec![]);
        }
        assert_eq!(
            name_rules(json!(" pdf processing "), None),
            vec![SkillRule::NameInvalidCharacters]
        );
        assert_eq!(
            name_rules(json!(format!(" {} ", "a".repeat(NAME_MAX_LENGTH))), None),
            vec![]
        );
        assert_eq!(
            name_rules(json!(" -pdf "), None),
            vec![SkillRule::NameEdgeHyphen]
        );
    }

    #[test]
    fn name_must_not_start_or_end_with_a_hyphen() {
        for name in ["-pdf", "pdf-", "-"] {
            assert_eq!(
                name_rules(json!(name), None),
                vec![SkillRule::NameEdgeHyphen]
            );
        }
    }

    #[test]
    fn name_must_not_contain_consecutive_hyphens() {
        for name in ["pdf--processing", "pdf---processing"] {
            assert_eq!(
                name_rules(json!(name), None),
                vec![SkillRule::NameDoubleHyphen]
            );
        }
    }

    #[test]
    fn name_reports_every_broken_rule_at_once() {
        assert_eq!(
            name_rules(json!("-Bad--Name_"), None),
            vec![
                SkillRule::NameNotLowercase,
                SkillRule::NameEdgeHyphen,
                SkillRule::NameDoubleHyphen,
                SkillRule::NameInvalidCharacters
            ]
        );
    }

    #[test]
    fn folder_is_not_checked_without_a_folder_name() {
        assert_eq!(name_rules(json!("anything"), None), vec![]);
    }

    #[test]
    fn folder_passes_when_it_matches_the_folder_name() {
        assert_eq!(
            name_rules(json!("pdf-processing"), Some("pdf-processing")),
            vec![]
        );
    }

    #[test]
    fn folder_reports_a_mismatch_naming_the_folder() {
        assert_eq!(
            check_skill_frontmatter(
                &fields(json!({"name": "pdf-processing", "description": "Does a thing."})),
                Some("pdf-tools")
            ),
            vec![SkillProblem {
                field: CheckedField::Name,
                rule: SkillRule::NameFolderMismatch,
                message: "name must match its folder name, \"pdf-tools\"".to_string()
            }]
        );
    }

    #[test]
    fn folder_compares_the_trimmed_name() {
        assert_eq!(name_rules(json!(" pdf "), Some("pdf")), vec![]);
    }

    #[test]
    fn folder_compares_case_sensitively() {
        assert_eq!(
            name_rules(json!("pdf"), Some("PDF")),
            vec![SkillRule::NameFolderMismatch]
        );
    }

    #[test]
    fn folder_compares_after_unicode_compatibility_normalization() {
        assert_eq!(name_rules(json!("cafe\u{301}"), Some("caf\u{e9}")), vec![]);
        assert_eq!(
            name_rules(json!("a\u{e7}\u{e3}o"), Some("a\u{e7}\u{e3}o")),
            vec![]
        );
    }

    #[test]
    fn folder_also_reports_a_mismatch_for_an_otherwise_invalid_name() {
        assert_eq!(
            name_rules(json!("Pdf"), Some("pdf")),
            vec![SkillRule::NameNotLowercase, SkillRule::NameFolderMismatch]
        );
    }

    #[test]
    fn folder_has_nothing_to_compare_when_the_name_is_missing_or_empty() {
        assert_eq!(
            name_rules(Value::Null, Some("pdf")),
            vec![SkillRule::NameMissing]
        );
        assert_eq!(
            name_rules(json!(""), Some("pdf")),
            vec![SkillRule::NameEmpty]
        );
    }

    #[test]
    fn folder_treats_an_empty_folder_name_as_a_name_to_match() {
        assert_eq!(
            name_rules(json!("pdf"), Some("")),
            vec![SkillRule::NameFolderMismatch]
        );
    }

    fn rules_with(compatibility: Value) -> Vec<SkillRule> {
        rules_of(
            with(
                Value::Object(valid()),
                json!({"compatibility": compatibility}),
            ),
            None,
        )
    }

    #[test]
    fn compatibility_takes_its_limit_from_the_specification() {
        assert_eq!(COMPATIBILITY_MAX_LENGTH, 500);
    }

    #[test]
    fn compatibility_is_optional() {
        assert_eq!(rules_of(Value::Object(valid()), None), vec![]);
    }

    #[test]
    fn compatibility_accepts_text_of_1_to_500_characters() {
        assert_eq!(rules_with(json!("Requires git and jq")), vec![]);
        assert_eq!(rules_with(json!("x")), vec![]);
        assert_eq!(
            rules_with(json!("x".repeat(COMPATIBILITY_MAX_LENGTH))),
            vec![]
        );
    }

    #[test]
    fn compatibility_counts_characters_not_utf16_units() {
        assert_eq!(
            rules_with(json!("😀".repeat(COMPATIBILITY_MAX_LENGTH))),
            vec![]
        );
        assert_eq!(
            rules_with(json!("😀".repeat(COMPATIBILITY_MAX_LENGTH + 1))),
            vec![SkillRule::CompatibilityTooLong]
        );
    }

    #[test]
    fn compatibility_reports_text_over_the_limit_with_the_counts() {
        let problems = check_skill_frontmatter(
            &fields(with(
                Value::Object(valid()),
                json!({"compatibility": "x".repeat(COMPATIBILITY_MAX_LENGTH + 1)}),
            )),
            None,
        );
        assert_eq!(
            problems,
            vec![SkillProblem {
                field: CheckedField::Compatibility,
                rule: SkillRule::CompatibilityTooLong,
                message: "compatibility has 501 characters; the limit is 500".to_string()
            }]
        );
    }

    #[test]
    fn compatibility_reports_empty_strings_blank_text_and_a_key_with_no_value_as_empty() {
        for value in [json!(""), json!("   \n"), Value::Null] {
            assert_eq!(rules_with(value), vec![SkillRule::CompatibilityEmpty]);
        }
    }

    #[test]
    fn compatibility_reports_a_number_true_a_list_and_a_map_as_not_text() {
        for value in [
            json!(3),
            json!(true),
            json!(["git", "jq"]),
            json!({"git": true}),
        ] {
            assert_eq!(rules_with(value), vec![SkillRule::CompatibilityNotText]);
        }
    }

    #[test]
    fn compatibility_is_listed_after_the_name_and_description_problems() {
        assert_eq!(
            rules_of(
                json!({"name": "Bad", "description": "", "compatibility": ""}),
                None
            ),
            vec![
                SkillRule::NameNotLowercase,
                SkillRule::DescriptionEmpty,
                SkillRule::CompatibilityEmpty
            ]
        );
    }

    #[test]
    fn compatibility_does_not_change_the_checks_of_name_and_description() {
        assert_eq!(check_skill_name("pdf-processing", None), vec![]);
    }

    fn name_only_rules(name: &str, folder: Option<&str>) -> Vec<SkillRule> {
        check_skill_name(name, folder)
            .into_iter()
            .map(|problem| problem.rule)
            .collect()
    }

    #[test]
    fn check_skill_name_has_no_problems_for_a_valid_name_with_or_without_a_folder() {
        assert_eq!(name_only_rules("pdf-processing", None), vec![]);
        assert_eq!(
            name_only_rules("pdf-processing", Some("pdf-processing")),
            vec![]
        );
    }

    #[test]
    fn check_skill_name_reports_an_empty_name_and_a_blank_one() {
        assert_eq!(name_only_rules("", None), vec![SkillRule::NameEmpty]);
        assert_eq!(name_only_rules("   ", None), vec![SkillRule::NameEmpty]);
    }

    #[test]
    fn check_skill_name_applies_the_same_name_rules_as_the_frontmatter_check() {
        assert_eq!(
            name_only_rules("PDF--Tool-", None),
            vec![
                SkillRule::NameNotLowercase,
                SkillRule::NameEdgeHyphen,
                SkillRule::NameDoubleHyphen
            ]
        );
        assert_eq!(
            name_only_rules("a/b", None),
            vec![SkillRule::NameInvalidCharacters]
        );
        assert_eq!(
            name_only_rules(&"a".repeat(65), None),
            vec![SkillRule::NameTooLong]
        );
    }

    #[test]
    fn check_skill_name_judges_the_trimmed_name() {
        assert_eq!(name_only_rules("  pdf-processing  ", None), vec![]);
    }

    #[test]
    fn check_skill_name_reports_a_folder_that_differs_from_the_name() {
        assert_eq!(
            name_only_rules("pdf-processing", Some("pdf")),
            vec![SkillRule::NameFolderMismatch]
        );
    }

    #[test]
    fn check_skill_name_agrees_with_check_skill_frontmatter_on_the_name_field() {
        for name in ["pdf", "Pdf", "-pdf", "p--df", "pdf!", ""] {
            assert_eq!(
                check_skill_name(name, Some("pdf")),
                check_skill_frontmatter(
                    &fields(json!({"name": name, "description": "Does a thing."})),
                    Some("pdf")
                ),
                "{name}"
            );
        }
    }
}
