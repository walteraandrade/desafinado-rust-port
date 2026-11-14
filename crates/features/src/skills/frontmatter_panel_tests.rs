use std::collections::HashSet;

use serde_json::json;

use super::frontmatter_fields::{FrontmatterProblem, FrontmatterProblemRule};
use super::frontmatter_panel::{
    FrontmatterContext, FrontmatterReport, PanelProblem, PanelRule, announce_problems,
    announce_rule, describe_frontmatter_problem, describe_skill_problem, inspect_frontmatter,
    is_same_report, other_problems, problems_of_field, report_problems,
};
use super::skill_frontmatter::{
    CheckedField, DESCRIPTION_MAX_LENGTH, NAME_MAX_LENGTH, SkillProblem, SkillRule,
    check_skill_frontmatter,
};

const SKILL_FILE: FrontmatterContext = FrontmatterContext {
    file_name: Some("SKILL.md"),
    folder_name: None,
};

const NOTES_FILE: FrontmatterContext = FrontmatterContext {
    file_name: Some("notes.md"),
    folder_name: None,
};

const NO_FILE: FrontmatterContext = FrontmatterContext {
    file_name: None,
    folder_name: None,
};

fn document_of(yaml: &str) -> String {
    format!("---\n{yaml}\n---\n\n# Title\n\nBody.\n")
}

fn valid() -> String {
    document_of("name: demo\ndescription: Does a thing.")
}

fn skill_in_folder(folder: &str) -> FrontmatterContext<'_> {
    FrontmatterContext {
        file_name: Some("SKILL.md"),
        folder_name: Some(folder),
    }
}

struct SkillReport {
    name: String,
    description: String,
    present: bool,
    problems: Vec<PanelProblem>,
}

fn skill_report(report: FrontmatterReport) -> SkillReport {
    match report {
        FrontmatterReport::Skill {
            name,
            description,
            present,
            problems,
        } => SkillReport {
            name,
            description,
            present,
            problems,
        },
        other => panic!("{other:?}"),
    }
}

fn missing(field: CheckedField) -> SkillProblem {
    SkillProblem {
        field,
        rule: match field {
            CheckedField::Name => SkillRule::NameMissing,
            _ => SkillRule::DescriptionMissing,
        },
        message: format!("{} is required", field.as_str()),
    }
}

fn fields_of(report: &SkillReport) -> Vec<Option<CheckedField>> {
    report
        .problems
        .iter()
        .map(|problem| problem.field)
        .collect()
}

#[test]
fn inspect_reads_name_and_description_of_a_skill_md_with_no_problems() {
    assert_eq!(
        inspect_frontmatter(&valid(), &SKILL_FILE),
        FrontmatterReport::Skill {
            name: "demo".to_string(),
            description: "Does a thing.".to_string(),
            present: true,
            problems: vec![]
        }
    );
}

#[test]
fn inspect_shows_nothing_for_a_document_that_is_not_a_skill_md() {
    assert_eq!(
        inspect_frontmatter(&valid(), &NOTES_FILE),
        FrontmatterReport::None
    );
    assert_eq!(
        inspect_frontmatter(&valid(), &NO_FILE),
        FrontmatterReport::None
    );
    assert_eq!(
        inspect_frontmatter("# Plain\n", &NOTES_FILE),
        FrontmatterReport::None
    );
}

#[test]
fn inspect_applies_the_skill_rules_to_a_skill_md_only() {
    let text = document_of("title: A post");
    assert_eq!(
        inspect_frontmatter(&text, &NOTES_FILE),
        FrontmatterReport::None
    );
    assert_eq!(
        fields_of(&skill_report(inspect_frontmatter(&text, &SKILL_FILE))),
        vec![Some(CheckedField::Name), Some(CheckedField::Description)]
    );
}

#[test]
fn inspect_reports_a_skill_md_with_no_frontmatter_as_missing_name_and_description() {
    let report = skill_report(inspect_frontmatter("# Title\n", &SKILL_FILE));
    assert!(!report.present);
    assert_eq!(report.name, "");
    assert_eq!(report.description, "");
    assert_eq!(
        report.problems,
        vec![
            PanelProblem {
                field: Some(CheckedField::Name),
                rule: PanelRule::Skill(SkillRule::NameMissing),
                message: describe_skill_problem(&missing(CheckedField::Name)),
            },
            PanelProblem {
                field: Some(CheckedField::Description),
                rule: PanelRule::Skill(SkillRule::DescriptionMissing),
                message: describe_skill_problem(&missing(CheckedField::Description)),
            },
        ]
    );
}

#[test]
fn inspect_checks_the_name_against_the_folder_only_when_the_folder_is_known() {
    let text = document_of("name: demo\ndescription: Does a thing.");
    assert_eq!(
        skill_report(inspect_frontmatter(&text, &SKILL_FILE)).problems,
        vec![]
    );
    assert_eq!(
        skill_report(inspect_frontmatter(&text, &skill_in_folder("demo"))).problems,
        vec![]
    );
    let mismatch = skill_report(inspect_frontmatter(&text, &skill_in_folder("other"))).problems;
    assert_eq!(mismatch.len(), 1);
    assert_eq!(mismatch[0].field, Some(CheckedField::Name));
    assert!(mismatch[0].message.contains("\"other\""));
}

#[test]
fn inspect_shows_the_values_as_the_editor_holds_them() {
    let report = skill_report(inspect_frontmatter(
        &document_of("name: \"  spaced  \"\ndescription: |-\n  one\n  two"),
        &SKILL_FILE,
    ));
    assert_eq!(report.name, "  spaced  ");
    assert_eq!(report.description, "one\ntwo");
}

#[test]
fn inspect_shows_numbers_and_booleans_as_text_and_flags_them() {
    let report = skill_report(inspect_frontmatter(
        &document_of("name: 123\ndescription: true"),
        &SKILL_FILE,
    ));
    assert_eq!(report.name, "123");
    assert_eq!(report.description, "true");
    assert_eq!(
        fields_of(&report),
        vec![Some(CheckedField::Name), Some(CheckedField::Description)]
    );
}

#[test]
fn inspect_shows_lists_and_maps_as_empty_and_flags_them() {
    let report = skill_report(inspect_frontmatter(
        &document_of("name: [a, b]\ndescription:\n  key: value"),
        &SKILL_FILE,
    ));
    assert_eq!(report.name, "");
    assert_eq!(report.description, "");
    assert_eq!(report.problems.len(), 2);
}

#[test]
fn inspect_flags_a_name_and_a_description_over_their_limits() {
    let text = document_of(&format!(
        "name: {}\ndescription: {}",
        "a".repeat(NAME_MAX_LENGTH + 1),
        "d".repeat(DESCRIPTION_MAX_LENGTH + 1)
    ));
    assert_eq!(
        fields_of(&skill_report(inspect_frontmatter(&text, &SKILL_FILE))),
        vec![Some(CheckedField::Name), Some(CheckedField::Description)]
    );
}

#[test]
fn unreadable_reports_a_frontmatter_that_is_not_closed_in_any_document() {
    let text = "---\nname: demo\n\n# Title\n";
    assert_eq!(
        inspect_frontmatter(text, &NOTES_FILE),
        FrontmatterReport::Unreadable {
            skill: false,
            problems: vec![PanelProblem {
                field: None,
                rule: PanelRule::Frontmatter(FrontmatterProblemRule::FrontmatterNotClosed),
                message: describe_frontmatter_problem(&FrontmatterProblem {
                    rule: FrontmatterProblemRule::FrontmatterNotClosed,
                    message: "frontmatter not closed".to_string(),
                    line: None,
                    column: None
                }),
            }]
        }
    );
    assert!(matches!(
        inspect_frontmatter(text, &SKILL_FILE),
        FrontmatterReport::Unreadable { skill: true, .. }
    ));
}

#[test]
fn unreadable_reports_a_yaml_syntax_error_with_its_line() {
    let text = document_of("name: demo\ndescription: [unclosed");
    let FrontmatterReport::Unreadable { problems, .. } = inspect_frontmatter(&text, &NOTES_FILE)
    else {
        panic!("expected unreadable");
    };
    assert_eq!(problems.len(), 1);
    let message = &problems[0].message;
    let digits = message
        .strip_prefix("YAML error on line ")
        .and_then(|rest| rest.split_once(": "))
        .map(|(line, _)| line);
    assert!(
        digits.is_some_and(|line| !line.is_empty() && line.chars().all(|c| c.is_ascii_digit()))
    );
}

#[test]
fn unreadable_reports_a_block_that_is_not_yaml_in_a_skill_md_only() {
    let text = "---\nthis is prose\n---\n\nText.\n";
    assert_eq!(
        inspect_frontmatter(text, &NOTES_FILE),
        FrontmatterReport::None
    );
    assert!(matches!(
        inspect_frontmatter(text, &SKILL_FILE),
        FrontmatterReport::Unreadable { skill: true, .. }
    ));
}

#[test]
fn unreadable_is_quiet_about_a_horizontal_rule_that_is_no_frontmatter() {
    assert_eq!(
        inspect_frontmatter("---\n\nA rule above.\n", &NOTES_FILE),
        FrontmatterReport::None
    );
}

const RULES: [SkillRule; 16] = [
    SkillRule::NameMissing,
    SkillRule::NameNotText,
    SkillRule::NameEmpty,
    SkillRule::NameTooLong,
    SkillRule::DescriptionMissing,
    SkillRule::DescriptionNotText,
    SkillRule::DescriptionEmpty,
    SkillRule::DescriptionTooLong,
    SkillRule::CompatibilityNotText,
    SkillRule::CompatibilityEmpty,
    SkillRule::CompatibilityTooLong,
    SkillRule::NameNotLowercase,
    SkillRule::NameInvalidCharacters,
    SkillRule::NameEdgeHyphen,
    SkillRule::NameDoubleHyphen,
    SkillRule::NameFolderMismatch,
];

fn field_of(rule: SkillRule) -> CheckedField {
    let text = rule.as_str();
    if text.starts_with("description") {
        CheckedField::Description
    } else if text.starts_with("compatibility") {
        CheckedField::Compatibility
    } else {
        CheckedField::Name
    }
}

fn fields_json(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map,
        other => panic!("{other:?}"),
    }
}

#[test]
fn describe_skill_problem_covers_every_rule_the_checker_has() {
    let produced: HashSet<SkillRule> = [
        json!({}),
        json!({"name": 1, "description": 1}),
        json!({"name": "", "description": ""}),
        json!({"name": "A--b-/", "description": "x".repeat(1025)}),
        json!({"name": "a".repeat(65), "description": "x"}),
        json!({"name": "abc", "description": "x"}),
        json!({"name": "abc", "description": "x", "compatibility": 3}),
        json!({"name": "abc", "description": "x", "compatibility": ""}),
        json!({"name": "abc", "description": "x", "compatibility": "c".repeat(501)}),
    ]
    .into_iter()
    .flat_map(|fields| {
        check_skill_frontmatter(&fields_json(fields), Some("folder"))
            .into_iter()
            .map(|problem| problem.rule)
    })
    .collect();
    for rule in produced {
        assert!(RULES.contains(&rule));
    }
}

#[test]
fn describe_skill_problem_says_what_is_wrong_for_every_rule() {
    for rule in RULES {
        let text = describe_skill_problem(&SkillProblem {
            field: field_of(rule),
            rule,
            message: format!("message for {}", rule.as_str()),
        });
        assert!(!text.is_empty());
        assert!(text.contains(field_of(rule).as_str()), "{}", rule.as_str());
    }
}

#[test]
fn describe_skill_problem_tells_where_a_missing_field_is_used() {
    assert!(
        describe_skill_problem(&missing(CheckedField::Description))
            .to_lowercase()
            .contains("decide when")
    );
}

#[test]
fn describe_skill_problem_names_the_folder_in_a_mismatch_and_how_to_fix_it() {
    let text = describe_skill_problem(&SkillProblem {
        field: CheckedField::Name,
        rule: SkillRule::NameFolderMismatch,
        message: "name must match its folder name, \"pdf\"".to_string(),
    });
    assert!(text.contains("\"pdf\""));
    assert!(text.to_lowercase().contains("rename the folder"));
}

#[test]
fn describe_skill_problem_keeps_the_counts_of_a_length_problem() {
    let problems = check_skill_frontmatter(
        &fields_json(json!({"name": "a".repeat(70), "description": "x"})),
        None,
    );
    assert!(describe_skill_problem(&problems[0]).contains("70"));
}

fn frontmatter_problem(rule: FrontmatterProblemRule, message: &str) -> FrontmatterProblem {
    FrontmatterProblem {
        rule,
        message: message.to_string(),
        line: None,
        column: None,
    }
}

#[test]
fn describe_frontmatter_problem_describes_every_rule() {
    for rule in [
        FrontmatterProblemRule::FrontmatterNotClosed,
        FrontmatterProblemRule::FrontmatterNotRecognized,
        FrontmatterProblemRule::YamlSyntaxError,
        FrontmatterProblemRule::FrontmatterNotAMapping,
    ] {
        assert!(
            !describe_frontmatter_problem(&frontmatter_problem(rule, "raw message")).is_empty()
        );
    }
}

#[test]
fn describe_frontmatter_problem_puts_the_line_and_the_yaml_message_of_a_syntax_error() {
    assert_eq!(
        describe_frontmatter_problem(&FrontmatterProblem {
            rule: FrontmatterProblemRule::YamlSyntaxError,
            message: "Flow sequence in block collection must be sufficiently indented".to_string(),
            line: Some(4),
            column: Some(2),
        }),
        "YAML error on line 4: Flow sequence in block collection must be sufficiently indented"
    );
}

#[test]
fn describe_frontmatter_problem_still_describes_a_syntax_error_with_no_line() {
    assert_eq!(
        describe_frontmatter_problem(&frontmatter_problem(
            FrontmatterProblemRule::YamlSyntaxError,
            "Bad"
        )),
        "YAML error: Bad"
    );
}

#[test]
fn report_problems_lists_the_messages_of_a_report_in_order() {
    let broken = inspect_frontmatter("---\nname: demo\n", &NOTES_FILE);
    let fields = inspect_frontmatter(&document_of("name: Demo\ndescription: \"\""), &SKILL_FILE);
    assert_eq!(report_problems(&fields).len(), 2);
    assert!(report_problems(&fields)[0].contains("lowercase"));
    assert_eq!(report_problems(&broken).len(), 1);
    assert_eq!(
        report_problems(&FrontmatterReport::None),
        Vec::<String>::new()
    );
    assert_eq!(
        report_problems(&inspect_frontmatter(&valid(), &SKILL_FILE)),
        Vec::<String>::new()
    );
}

#[test]
fn problems_of_field_picks_the_problems_of_one_field() {
    let broken = inspect_frontmatter("---\nname: demo\n", &NOTES_FILE);
    let fields = inspect_frontmatter(&document_of("name: Demo\ndescription: \"\""), &SKILL_FILE);
    assert_eq!(problems_of_field(&fields, CheckedField::Name).len(), 1);
    assert_eq!(
        problems_of_field(&fields, CheckedField::Description).len(),
        1
    );
    assert_eq!(problems_of_field(&broken, CheckedField::Name), vec![]);
    assert_eq!(
        problems_of_field(&FrontmatterReport::None, CheckedField::Name),
        vec![]
    );
}

fn with_compatibility(value: &str) -> FrontmatterReport {
    inspect_frontmatter(
        &document_of(&format!(
            "name: demo\ndescription: Does a thing.\ncompatibility: {value}"
        )),
        &SKILL_FILE,
    )
}

#[test]
fn compatibility_is_no_problem_when_absent_or_valid() {
    assert!(matches!(
        with_compatibility("Requires git"),
        FrontmatterReport::Skill { ref problems, .. } if problems.is_empty()
    ));
    assert_eq!(other_problems(&with_compatibility("Requires git")), vec![]);
    assert_eq!(
        other_problems(&inspect_frontmatter(&valid(), &SKILL_FILE)),
        vec![]
    );
}

#[test]
fn compatibility_shows_a_problem_with_it_under_its_own_name() {
    let report = with_compatibility(&format!("\"{}\"", "c".repeat(501)));
    assert_eq!(
        problems_of_field(&report, CheckedField::Compatibility),
        vec![PanelProblem {
            field: Some(CheckedField::Compatibility),
            rule: PanelRule::Skill(SkillRule::CompatibilityTooLong),
            message: "compatibility has 501 characters; the limit is 500".to_string(),
        }]
    );
    assert_eq!(problems_of_field(&report, CheckedField::Name), vec![]);
    assert_eq!(
        problems_of_field(&report, CheckedField::Description),
        vec![]
    );
}

#[test]
fn compatibility_counts_among_the_problems_shown_by_save() {
    assert_eq!(
        report_problems(&with_compatibility("3")),
        vec!["compatibility must be text".to_string()]
    );
    assert_eq!(
        report_problems(&with_compatibility("\"\"")),
        vec!["compatibility is empty. Remove it or say what the skill needs".to_string()]
    );
}

#[test]
fn compatibility_is_listed_with_the_problems_no_input_shows() {
    assert_eq!(
        other_problems(&with_compatibility("3")),
        vec![PanelProblem {
            field: Some(CheckedField::Compatibility),
            rule: PanelRule::Skill(SkillRule::CompatibilityNotText),
            message: "compatibility must be text".to_string(),
        }]
    );
}

#[test]
fn compatibility_leaves_name_and_description_problems_to_their_inputs() {
    let broken = inspect_frontmatter("---\nname: demo\n", &NOTES_FILE);
    let fields = inspect_frontmatter(&document_of("name: Demo\ndescription: \"\""), &SKILL_FILE);
    assert_eq!(other_problems(&fields), vec![]);
    assert_eq!(other_problems(&broken).len(), 1);
    assert_eq!(other_problems(&FrontmatterReport::None), vec![]);
}

fn skill(name: &str, description: &str) -> FrontmatterReport {
    inspect_frontmatter(
        &document_of(&format!("name: {name}\ndescription: {description}")),
        &SKILL_FILE,
    )
}

#[test]
fn announce_says_there_are_no_problems_when_there_are_none() {
    assert_eq!(
        announce_problems(&skill("demo", "Does a thing.")),
        "No frontmatter problems"
    );
}

#[test]
fn announce_says_nothing_for_a_document_with_no_frontmatter_panel() {
    assert_eq!(announce_problems(&FrontmatterReport::None), "");
}

#[test]
fn announce_names_the_kinds_of_problem_in_order() {
    assert_eq!(
        announce_problems(&skill("Demo--", "")),
        "Frontmatter problems: name is not lowercase; name starts or ends with a hyphen; name has consecutive hyphens; description is missing"
    );
}

#[test]
fn announce_does_not_change_while_the_problems_are_the_same_kinds_with_other_numbers() {
    assert_eq!(
        announce_problems(&skill(&"a".repeat(70), "Does a thing.")),
        announce_problems(&skill(&"a".repeat(71), "Does a thing."))
    );
    assert_eq!(
        announce_problems(&skill("demo", &"d".repeat(1030))),
        announce_problems(&skill("demo", &"d".repeat(1031)))
    );
}

#[test]
fn announce_does_not_change_with_the_line_and_the_text_of_a_yaml_error() {
    let at = |text: &str| announce_problems(&inspect_frontmatter(text, &NOTES_FILE));
    assert_eq!(
        at("---\nname: [a\n---\n"),
        at("---\nname: demo\nx: [a\n---\n")
    );
    assert_eq!(
        at("---\nname: [a\n---\n"),
        "Frontmatter problems: the frontmatter has a YAML error"
    );
}

#[test]
fn announce_does_not_change_when_the_folder_in_a_mismatch_is_another_one() {
    let mismatch = |folder: &str| {
        announce_problems(&inspect_frontmatter(
            &document_of("name: demo\ndescription: Does it."),
            &skill_in_folder(folder),
        ))
    };
    assert_eq!(mismatch("one"), mismatch("two"));
}

#[test]
fn announce_changes_when_a_kind_of_problem_appears_or_goes() {
    let kinds: HashSet<String> = [
        announce_problems(&skill("demo", "Does a thing.")),
        announce_problems(&skill("Demo", "Does a thing.")),
        announce_problems(&skill("Demo", "")),
        announce_problems(&skill("demo", "")),
    ]
    .into_iter()
    .collect();
    assert_eq!(kinds.len(), 4);
}

#[test]
fn announce_has_a_short_phrase_with_no_digits_for_every_rule() {
    let rules: Vec<PanelRule> = RULES
        .iter()
        .map(|rule| PanelRule::Skill(*rule))
        .chain(
            [
                FrontmatterProblemRule::FrontmatterNotClosed,
                FrontmatterProblemRule::FrontmatterNotRecognized,
                FrontmatterProblemRule::FrontmatterNotAMapping,
                FrontmatterProblemRule::YamlSyntaxError,
            ]
            .map(PanelRule::Frontmatter),
        )
        .collect();
    for rule in &rules {
        let phrase = announce_rule(*rule);
        assert!(!phrase.is_empty());
        assert!(!phrase.chars().any(|c| c.is_ascii_digit()));
    }
    let distinct: HashSet<&str> = rules.iter().map(|rule| announce_rule(*rule)).collect();
    assert_eq!(distinct.len(), rules.len());
}

#[test]
fn is_same_report_is_true_for_equal_reports_built_twice() {
    assert!(is_same_report(
        &inspect_frontmatter(&valid(), &SKILL_FILE),
        &inspect_frontmatter(&valid(), &SKILL_FILE)
    ));
}

#[test]
fn is_same_report_is_false_when_a_value_or_a_problem_differs() {
    let other = document_of("name: demo\ndescription: Another.");
    assert!(!is_same_report(
        &inspect_frontmatter(&valid(), &SKILL_FILE),
        &inspect_frontmatter(&other, &SKILL_FILE)
    ));
    assert!(!is_same_report(
        &inspect_frontmatter(&valid(), &SKILL_FILE),
        &inspect_frontmatter(&valid(), &NOTES_FILE)
    ));
}
