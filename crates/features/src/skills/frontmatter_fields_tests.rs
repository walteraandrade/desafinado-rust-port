use serde_json::{Value, json};

use super::frontmatter_block::find_frontmatter;
use super::frontmatter_fields::{
    FrontmatterFields, FrontmatterProblemRule, FrontmatterReading, SkillFieldValues, TextEdit,
    apply_edits, plan_skill_field_edits, read_frontmatter, read_yaml_fields, set_skill_fields,
};

const BODY: &str = "\n# Demo\n\nBody has name: nope and a rule.\n\n---\n\nAfter.\n";

fn document_of(yaml: &str) -> String {
    format!("---\n{yaml}\n---\n{BODY}")
}

fn edited(text: &str, values: &SkillFieldValues) -> String {
    match set_skill_fields(text, values) {
        Ok(result) => result,
        Err(problem) => panic!("{}", problem.message),
    }
}

fn fields_of(text: &str) -> FrontmatterFields {
    match read_frontmatter(text) {
        FrontmatterReading::Present(fields) => fields,
        other => panic!("{other:?}"),
    }
}

fn body_of(text: &str) -> &str {
    &text[find_frontmatter(text).map_or(0, |found| found.to)..]
}

fn object(value: Value) -> FrontmatterFields {
    match value {
        Value::Object(map) => map,
        other => panic!("{other:?}"),
    }
}

fn invalid(text: &str) -> super::frontmatter_fields::FrontmatterProblem {
    match read_frontmatter(text) {
        FrontmatterReading::Invalid(problem) => problem,
        other => panic!("{other:?}"),
    }
}

fn name(value: &str) -> SkillFieldValues {
    SkillFieldValues::name(value)
}

fn description(value: &str) -> SkillFieldValues {
    SkillFieldValues::description(value)
}

fn both(name: &str, description: &str) -> SkillFieldValues {
    SkillFieldValues::both(name, description)
}

#[test]
fn read_frontmatter_reads_name_and_description() {
    assert_eq!(
        read_frontmatter(&document_of("name: demo\ndescription: Does a thing.")),
        FrontmatterReading::Present(object(
            json!({"name": "demo", "description": "Does a thing."})
        ))
    );
}

#[test]
fn read_frontmatter_keeps_the_other_keys_as_unknown_values() {
    let fields = fields_of(&document_of(
        "name: demo\nlicense: MIT\nversion: 2\nallowed-tools:\n  - Read\n  - Write\nmetadata:\n  author: me\n  tags: [a, b]",
    ));
    assert_eq!(
        fields,
        object(json!({
            "name": "demo",
            "license": "MIT",
            "version": 2,
            "allowed-tools": ["Read", "Write"],
            "metadata": {"author": "me", "tags": ["a", "b"]},
        }))
    );
}

#[test]
fn read_frontmatter_does_not_coerce_name_or_description_into_text() {
    assert_eq!(
        fields_of(&document_of("name: 123\ndescription: true")),
        object(json!({"name": 123, "description": true}))
    );
    assert_eq!(
        fields_of(&document_of("name:\ndescription: [a]")),
        object(json!({"name": null, "description": ["a"]}))
    );
}

#[test]
fn read_frontmatter_reads_block_scalars_quotes_and_comments() {
    let fields = fields_of(&document_of(
        "# lead\nname: \"demo\" # trailing\ndescription: >\n  Folded\n  text: with a colon.\n",
    ));
    assert_eq!(
        fields,
        object(json!({"name": "demo", "description": "Folded text: with a colon.\n"}))
    );
}

#[test]
fn read_frontmatter_reads_an_empty_frontmatter_as_no_fields() {
    assert_eq!(
        read_frontmatter("---\n---\n\nBody.\n"),
        FrontmatterReading::Present(FrontmatterFields::new())
    );
    assert_eq!(
        read_frontmatter("---\n\n---\n"),
        FrontmatterReading::Present(FrontmatterFields::new())
    );
}

#[test]
fn read_frontmatter_reports_a_document_without_frontmatter_as_absent() {
    assert_eq!(
        read_frontmatter("# Title\n\nBody.\n"),
        FrontmatterReading::Absent
    );
    assert_eq!(read_frontmatter(""), FrontmatterReading::Absent);
}

#[test]
fn read_frontmatter_treats_a_rule_followed_by_prose_as_absent_not_as_unclosed() {
    assert_eq!(
        read_frontmatter("---\n\nIntro.\n\nMore.\n"),
        FrontmatterReading::Absent
    );
    assert_eq!(read_frontmatter("---\n"), FrontmatterReading::Absent);
    assert_eq!(
        read_frontmatter("---\n# only a comment\n"),
        FrontmatterReading::Absent
    );
}

#[test]
fn read_frontmatter_reports_an_unclosed_frontmatter() {
    let problem = invalid("---\nname: demo\n\n# Demo\n\nBody.\n");
    assert_eq!(problem.rule, FrontmatterProblemRule::FrontmatterNotClosed);
    assert_eq!(problem.message, "frontmatter not closed");
}

#[test]
fn read_frontmatter_reports_an_unclosed_frontmatter_followed_by_many_blank_lines() {
    assert_eq!(
        invalid("---\nname: demo\n\n\n\n").rule,
        FrontmatterProblemRule::FrontmatterNotClosed
    );
}

#[test]
fn read_frontmatter_reports_an_unclosed_frontmatter_at_the_very_end_of_the_document() {
    assert_eq!(
        invalid("---\nname: demo").rule,
        FrontmatterProblemRule::FrontmatterNotClosed
    );
    assert_eq!(
        invalid("---\nname: demo\n").rule,
        FrontmatterProblemRule::FrontmatterNotClosed
    );
}

#[test]
fn read_frontmatter_reports_a_malformed_closing_delimiter_as_unclosed() {
    assert_eq!(
        invalid("---\nname: demo\n----\n\nBody.\n").rule,
        FrontmatterProblemRule::FrontmatterNotClosed
    );
}

#[test]
fn read_frontmatter_reports_a_yaml_syntax_error_with_its_document_line() {
    let problem = invalid(&document_of(
        "name: demo\ndescription: [unclosed\nlicense: MIT",
    ));
    assert_eq!(problem.rule, FrontmatterProblemRule::YamlSyntaxError);
    assert_ne!(problem.message, "");
    assert!(!problem.message.contains('\n'));
    assert!(problem.line.is_some_and(|line| line >= 3));
    assert!(problem.column.is_some_and(|column| column >= 1));
}

#[test]
fn read_frontmatter_reports_a_bad_indentation_as_a_syntax_error() {
    assert_eq!(
        invalid(&document_of(
            "name: demo\n  description: nested\nlicense: x"
        ))
        .rule,
        FrontmatterProblemRule::YamlSyntaxError
    );
}

#[test]
fn read_frontmatter_reports_a_duplicate_key_as_a_syntax_error() {
    let problem = invalid(&document_of("name: one\ndescription: x\nname: two"));
    assert_eq!(problem.rule, FrontmatterProblemRule::YamlSyntaxError);
    assert_eq!(problem.line, Some(4));
}

#[test]
fn read_frontmatter_reports_a_tab_used_as_indentation() {
    assert_eq!(
        invalid(&document_of("name: demo\nmetadata:\n\tauthor: me")).rule,
        FrontmatterProblemRule::YamlSyntaxError
    );
}

#[test]
fn read_frontmatter_does_not_throw_on_an_alias_bomb() {
    let yaml = [
        "a: &a [x, x, x, x, x, x, x, x, x, x]",
        "b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a, *a]",
        "c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b, *b]",
        "d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c, *c]",
        "e: &e [*d, *d, *d, *d, *d, *d, *d, *d, *d, *d]",
    ]
    .join("\n");
    assert!(matches!(
        read_frontmatter(&document_of(&yaml)),
        FrontmatterReading::Invalid(_)
    ));
}

#[test]
fn read_frontmatter_reads_fields_after_a_body_that_looks_like_frontmatter() {
    assert_eq!(
        fields_of(&document_of("name: demo")),
        object(json!({"name": "demo"}))
    );
}

#[test]
fn read_yaml_fields_reads_a_mapping() {
    assert_eq!(
        read_yaml_fields("name: demo\nn: 1", 1),
        Ok(object(json!({"name": "demo", "n": 1})))
    );
}

#[test]
fn read_yaml_fields_reads_empty_yaml_and_comments_as_no_fields() {
    assert_eq!(read_yaml_fields("", 1), Ok(FrontmatterFields::new()));
    assert_eq!(
        read_yaml_fields("# only a comment\n", 1),
        Ok(FrontmatterFields::new())
    );
}

#[test]
fn read_yaml_fields_reports_a_sequence_as_not_a_mapping() {
    assert_eq!(
        read_yaml_fields("- a\n- b", 1).map_err(|problem| problem.rule),
        Err(FrontmatterProblemRule::FrontmatterNotAMapping)
    );
}

#[test]
fn read_yaml_fields_reports_a_bare_scalar_as_not_a_mapping() {
    assert_eq!(
        read_yaml_fields("just text", 1).map_err(|problem| problem.rule),
        Err(FrontmatterProblemRule::FrontmatterNotAMapping)
    );
}

#[test]
fn read_yaml_fields_numbers_lines_from_the_given_first_line() {
    let at = |first_line: usize| {
        read_yaml_fields("a: 1\nb: [", first_line)
            .err()
            .and_then(|problem| problem.line)
    };
    assert_eq!(at(1), Some(2));
    assert_eq!(at(2), Some(3));
}

#[test]
fn replacing_values_changes_only_the_frontmatter() {
    let text = document_of("name: old\ndescription: Old text.");
    let result = edited(&text, &both("new-name", "New text."));
    assert_eq!(
        result,
        document_of("name: new-name\ndescription: New text.")
    );
    assert_eq!(body_of(&result), body_of(&text));
}

#[test]
fn replacing_values_changes_one_field_and_leaves_the_other_line_alone() {
    let text = document_of("name: old\ndescription: Old text.");
    assert_eq!(
        edited(&text, &name("new")),
        document_of("name: new\ndescription: Old text.")
    );
    assert_eq!(
        edited(&text, &description("New.")),
        document_of("name: old\ndescription: New.")
    );
}

#[test]
fn replacing_values_returns_the_same_text_when_nothing_changes() {
    let text = document_of(
        "# lead\nname:   'demo'   # keep\ndescription: >\n  Folded\n  text\nextra:\n- a\n- b",
    );
    assert_eq!(edited(&text, &SkillFieldValues::default()), text);
    assert_eq!(edited(&text, &name("demo")), text);
    assert_eq!(edited(&text, &both("demo", "Folded text\n")), text);
}

#[test]
fn replacing_values_keeps_the_order_of_the_keys() {
    let text = document_of("license: MIT\ndescription: d\nname: n\nz: 1");
    assert_eq!(
        edited(&text, &both("x", "y")),
        document_of("license: MIT\ndescription: y\nname: x\nz: 1")
    );
}

#[test]
fn replacing_values_keeps_comments_blank_lines_and_the_other_keys_byte_for_byte() {
    let yaml = [
        "# Managed by hand",
        "",
        "name: old # the name",
        "",
        "# about the description",
        "description: Old text. # why",
        "license:    MIT   # spacing",
        "allowed-tools:",
        "- Read",
        "- Write",
        "notes: first line",
        "  second line",
        "quoted: 'single'",
        "metadata:",
        "    author:   me",
        "# end comment",
    ]
    .join("\n");
    let result = edited(&document_of(&yaml), &both("new-name", "New text."));
    assert_eq!(
        result,
        document_of(
            &yaml
                .replace("name: old # the name", "name: new-name # the name")
                .replace(
                    "description: Old text. # why",
                    "description: New text. # why"
                )
        )
    );
}

#[test]
fn replacing_values_replaces_a_multi_line_plain_value_entirely() {
    let text = document_of("name: n\ndescription: line one\n  line two\nz: 1");
    assert_eq!(
        edited(&text, &description("short")),
        document_of("name: n\ndescription: short\nz: 1")
    );
}

#[test]
fn replacing_values_replaces_a_value_that_sits_on_the_next_line() {
    let text = document_of("name: n\ndescription:\n  text on next line\nz: 1");
    assert_eq!(
        edited(&text, &description("short")),
        document_of("name: n\ndescription: short\nz: 1")
    );
}

#[test]
fn replacing_values_fills_an_empty_value() {
    assert_eq!(
        edited(&document_of("name:\ndescription: d"), &name("n")),
        document_of("name: n\ndescription: d")
    );
    assert_eq!(
        edited(&document_of("name: n\ndescription:"), &description("d")),
        document_of("name: n\ndescription: d")
    );
}

#[test]
fn replacing_values_keeps_a_comment_on_an_empty_value() {
    assert_eq!(
        edited(&document_of("name: # fill me\ndescription: d"), &name("n")),
        document_of("name: n # fill me\ndescription: d")
    );
}

#[test]
fn replacing_values_replaces_a_value_that_is_not_text() {
    let text = document_of("name: 123\ndescription: [a, b]\nz: 1");
    assert_eq!(
        edited(&text, &both("n", "d")),
        document_of("name: n\ndescription: d\nz: 1")
    );
}

#[test]
fn replacing_values_replaces_a_block_sequence_value() {
    let text = document_of("name: n\ndescription:\n  - a\n  - b\nz: 1");
    assert_eq!(
        edited(&text, &description("d")),
        document_of("name: n\ndescription: d\nz: 1")
    );
}

#[test]
fn replacing_values_keeps_a_quoted_key() {
    let text = document_of("\"name\": old\n'description': old");
    assert_eq!(
        edited(&text, &both("n", "d")),
        document_of("\"name\": n\n'description': d")
    );
}

#[test]
fn replacing_values_keeps_the_blank_lines_after_a_block_value() {
    let text = document_of("name: n\ndescription: |\n  old\n\n\nz: 1");
    assert_eq!(
        edited(&text, &description("new")),
        document_of("name: n\ndescription: |-\n  new\n\n\nz: 1")
    );
}

#[test]
fn replacing_values_replaces_a_block_value_at_the_end_of_the_frontmatter() {
    let text = document_of("name: n\ndescription: >-\n  old\n  text");
    assert_eq!(
        edited(&text, &description("new")),
        document_of("name: n\ndescription: >-\n  new")
    );
}

#[test]
fn replacing_values_handles_non_ascii_text_around_the_value() {
    let text = document_of("name: café\ndescription: 日本語 \u{1F600}\nz: ü");
    assert_eq!(
        edited(&text, &both("é", "ö")),
        document_of("name: é\ndescription: ö\nz: ü")
    );
}

#[test]
fn styles_keep_quotes_around_a_single_line_value() {
    assert_eq!(
        edited(
            &document_of("name: \"old\"\ndescription: 'old'"),
            &both("new", "new")
        ),
        document_of("name: \"new\"\ndescription: 'new'")
    );
}

#[test]
fn styles_keep_the_folded_and_literal_block_styles() {
    assert_eq!(
        edited(
            &document_of("name: n\ndescription: >\n  old\n  text"),
            &description("new text")
        ),
        document_of("name: n\ndescription: >-\n  new text")
    );
    assert_eq!(
        edited(
            &document_of("name: n\ndescription: |\n  old"),
            &description("new text")
        ),
        document_of("name: n\ndescription: |-\n  new text")
    );
}

#[test]
fn styles_write_a_multi_line_value_as_a_literal_block() {
    assert_eq!(
        edited(
            &document_of("name: n\ndescription: old"),
            &description("line one\nline two")
        ),
        document_of("name: n\ndescription: |-\n  line one\n  line two")
    );
}

#[test]
fn styles_switch_a_quoted_value_to_a_literal_block_when_it_gets_lines() {
    assert_eq!(
        edited(
            &document_of("name: n\ndescription: \"old\""),
            &description("a\nb")
        ),
        document_of("name: n\ndescription: |-\n  a\n  b")
    );
}

#[test]
fn styles_keep_a_folded_block_when_the_value_gets_lines() {
    let result = edited(
        &document_of("name: n\ndescription: >\n  old"),
        &description("a\nb"),
    );
    assert_eq!(fields_of(&result)["description"], json!("a\nb"));
    assert!(result.contains("description: >"));
}

#[test]
fn styles_keep_a_trailing_newline_in_a_block_value() {
    let result = edited(
        &document_of("name: n\ndescription: old"),
        &description("line one\nline two\n"),
    );
    assert_eq!(
        result,
        document_of("name: n\ndescription: |\n  line one\n  line two")
    );
    assert_eq!(
        fields_of(&result)["description"],
        json!("line one\nline two\n")
    );
}

#[test]
fn styles_do_not_fold_a_long_value() {
    let long = format!("{}end", "word ".repeat(60));
    let result = edited(
        &document_of("name: n\ndescription: old"),
        &description(&long),
    );
    assert_eq!(
        result,
        document_of(&format!("name: n\ndescription: {long}"))
    );
}

#[test]
fn styles_do_not_fold_a_long_quoted_value() {
    let long = format!("{}end", "word: ".repeat(60));
    let result = edited(
        &document_of("name: n\ndescription: old"),
        &description(&long),
    );
    assert_eq!(
        result,
        document_of(&format!("name: n\ndescription: \"{long}\""))
    );
}

#[test]
fn styles_move_a_trailing_comment_into_the_header_of_a_block_value() {
    let result = edited(
        &document_of("name: n\ndescription: old # why"),
        &description("a\nb"),
    );
    assert_eq!(
        result,
        document_of("name: n\ndescription: |- # why\n  a\n  b")
    );
}

#[test]
fn styles_keep_a_header_comment_of_a_block_value() {
    let result = edited(
        &document_of("name: n\ndescription: > # why\n  old\nz: 1"),
        &description("new"),
    );
    assert_eq!(
        result,
        document_of("name: n\ndescription: >- # why\n  new\nz: 1")
    );
}

#[test]
fn styles_keep_a_comment_between_the_key_and_its_value() {
    let result = edited(
        &document_of("name: n\ndescription:\n  # note\n  old\nz: 1"),
        &description("new"),
    );
    assert!(result.contains("# note"));
    assert_eq!(
        fields_of(&result),
        object(json!({"name": "n", "description": "new", "z": 1}))
    );
}

#[test]
fn styles_keep_an_anchor_so_its_aliases_still_resolve() {
    let result = edited(
        &document_of("name: n\ndescription: &d old\nz: *d"),
        &description("new"),
    );
    assert_eq!(result, document_of("name: n\ndescription: &d new\nz: *d"));
    assert_eq!(
        fields_of(&result),
        object(json!({"name": "n", "description": "new", "z": "new"}))
    );
}

#[test]
fn styles_do_not_let_a_block_value_absorb_the_blank_lines_after_it() {
    let text = document_of("name: n\ndescription: |\n  old\n\nz: 1");
    let result = edited(&text, &description("new\n\n"));
    assert_eq!(
        fields_of(&result),
        object(json!({"name": "n", "description": "new\n\n", "z": 1}))
    );
    assert!(result.contains("\nz: 1\n---"));
}

#[test]
fn styles_do_not_let_a_block_value_keep_a_whitespace_only_last_line() {
    let text = document_of("name: n\ndescription: old\n\nz: 1");
    let result = edited(&text, &description("a\n\t\n"));
    assert_eq!(
        fields_of(&result),
        object(json!({"name": "n", "description": "a\n\t\n", "z": 1}))
    );
}

#[test]
fn styles_write_a_whitespace_only_value_in_quotes_not_as_a_block() {
    let result = edited(
        &document_of("name: n\ndescription: |\n  old"),
        &description(" "),
    );
    assert_eq!(result, document_of("name: n\ndescription: \" \""));
}

#[test]
fn styles_fall_back_to_a_json_string_when_no_block_or_quote_reads_back() {
    let value = "\u{0}null\n\n\n\n~\n\n\n\n\n \n-?true";
    let result = edited(&document_of("z: 1"), &description(value));
    assert_eq!(
        result,
        document_of("description: \"\\u0000null\\n\\n\\n\\n~\\n\\n\\n\\n\\n \\n-?true\"\nz: 1")
    );
    assert_eq!(
        fields_of(&result),
        object(json!({"description": value, "z": 1}))
    );
}

#[test]
fn styles_escape_unicode_line_breaks_in_the_json_fallback() {
    let value = "\u{0}null\n\n\n\n~\n\n\n\n\n \n-?true\u{2028}\u{85}\u{2029}";
    let result = edited(&document_of("z: 1"), &description(value));
    assert!(result.contains("-?true\\u2028\\u0085\\u2029\""));
    assert_eq!(
        fields_of(&result),
        object(json!({"description": value, "z": 1}))
    );
}

const TRICKY: &[&str] = &[
    "Does: a thing",
    "Has a # hash",
    "Ends with colon:",
    "Starts # with hash",
    "# starts with a hash",
    "- starts with a dash",
    "-",
    "?",
    "? question",
    ": colon",
    "it's here",
    "say \"hi\"",
    "both ' and \"",
    "[bracket] and {brace}",
    "[start",
    "{start",
    "*alias",
    "&anchor",
    "!tag",
    "|pipe",
    ">gt",
    "%percent",
    "@at",
    "`tick`",
    "\\backslash",
    "tab\there",
    " leading space",
    "trailing space ",
    "  both  ",
    "true",
    "false",
    "null",
    "~",
    "123",
    "1.5",
    "0x1F",
    "2024-01-01",
    "yes",
    "no",
    "---",
    "...",
    "--- text",
    "café 日本語 \u{1F600}",
    "x: y: z",
    "a, b, c",
    "url https://example.com/a#b",
    "",
    " ",
];

const MULTILINE: &[&str] = &[
    "a\nb",
    "a\n\nb",
    "a\n\n\nb",
    "\nleading",
    "\n\nleading",
    "trailing\n",
    "trailing\n\n",
    "trailing\n\n\n",
    "\n",
    "\n\n",
    "a\n",
    "  indented first\nsecond",
    "first\n  indented second",
    "first\n\tindented with tab",
    "first \nsecond",
    "first\n second",
    "first\n  \nthird",
    "# not a comment\nb",
    "a: b\nc: d",
    "- a\n- b",
    "---\ntext\n---",
    "key: value\n\n  nested: x",
    "a\u{2028}b",
    "a\u{85}b",
    "quote \" and ' here\nnext",
    "trailing space \nnext ",
];

#[test]
fn special_characters_round_trip_in_a_plain_value() {
    for value in TRICKY {
        let result = edited(
            &document_of("name: n\ndescription: old\nz: 1"),
            &description(value),
        );
        assert_eq!(
            fields_of(&result),
            object(json!({"name": "n", "description": value, "z": 1})),
            "{value:?}"
        );
        assert_eq!(body_of(&result), format!("\n{BODY}"), "{value:?}");
    }
}

#[test]
fn special_characters_round_trip_in_a_quoted_value() {
    for value in TRICKY {
        let result = edited(
            &document_of("name: \"n\"\ndescription: \"old\"\nz: 1"),
            &both(value, value),
        );
        assert_eq!(
            fields_of(&result),
            object(json!({"name": value, "description": value, "z": 1})),
            "{value:?}"
        );
    }
}

#[test]
fn special_characters_round_trip_in_a_block_value() {
    for value in TRICKY {
        let result = edited(
            &document_of("name: n\ndescription: |\n  old\nz: 1"),
            &description(value),
        );
        assert_eq!(
            fields_of(&result),
            object(json!({"name": "n", "description": value, "z": 1})),
            "{value:?}"
        );
    }
}

#[test]
fn special_characters_round_trip_when_inserted() {
    for value in TRICKY {
        let result = edited(&document_of("z: 1"), &both(value, value));
        assert_eq!(
            fields_of(&result),
            object(json!({"name": value, "description": value, "z": 1})),
            "{value:?}"
        );
    }
}

#[test]
fn multi_line_values_round_trip_as_a_multi_line_value() {
    for value in MULTILINE {
        let result = edited(
            &document_of("name: n\ndescription: old\nz: 1"),
            &description(value),
        );
        assert_eq!(
            fields_of(&result),
            object(json!({"name": "n", "description": value, "z": 1})),
            "{value:?}"
        );
        assert_eq!(body_of(&result), format!("\n{BODY}"), "{value:?}");
    }
}

#[test]
fn multi_line_values_round_trip_over_a_folded_block() {
    for value in MULTILINE {
        let result = edited(
            &document_of("name: n\ndescription: >\n  old\nz: 1"),
            &description(value),
        );
        assert_eq!(
            fields_of(&result),
            object(json!({"name": "n", "description": value, "z": 1})),
            "{value:?}"
        );
    }
}

#[test]
fn multi_line_values_round_trip_when_it_is_last() {
    for value in MULTILINE {
        let result = edited(
            &document_of("name: n\ndescription: old"),
            &description(value),
        );
        assert_eq!(
            fields_of(&result),
            object(json!({"name": "n", "description": value})),
            "{value:?}"
        );
        assert_eq!(body_of(&result), format!("\n{BODY}"), "{value:?}");
    }
}

#[test]
fn multi_line_values_round_trip_after_a_trailing_comment() {
    for value in MULTILINE {
        let result = edited(
            &document_of("name: n\ndescription: old # why\nz: 1"),
            &description(value),
        );
        assert_eq!(
            fields_of(&result),
            object(json!({"name": "n", "description": value, "z": 1})),
            "{value:?}"
        );
    }
}

#[test]
fn round_trips_random_values() {
    let pieces: Vec<&str> = vec![
        "a",
        "Z",
        "1",
        " ",
        "  ",
        "\n",
        "\n\n",
        ":",
        ": ",
        "#",
        " #",
        "\"",
        "'",
        "-",
        "- ",
        "?",
        "[",
        "]",
        "{",
        "}",
        ",",
        "&",
        "*",
        "!",
        "|",
        ">",
        "%",
        "@",
        "`",
        "\\",
        "\t",
        "é",
        "日",
        "\u{1F600}",
        "~",
        "---",
        "...",
        "\u{7}",
        "\u{2028}",
        "\u{85}",
        "\u{feff}",
    ];
    let mut seed: u64 = 20260;
    let mut next = || {
        seed = (seed * 1_664_525 + 1_013_904_223) % 4_294_967_296;
        seed as f64 / 4_294_967_296.0
    };
    let mut random = || {
        let length = (next() * 14.0).floor() as usize;
        (0..length)
            .map(|_| pieces[(next() * pieces.len() as f64).floor() as usize])
            .collect::<String>()
    };
    let bases = [
        "name: n\ndescription: old\nz: 1",
        "name: \"n\"\ndescription: \"old\" # c\nz: 1",
        "name: 'n'\ndescription: 'old'",
        "name: n\ndescription: |\n  old\n\nz: 1",
        "name: n\ndescription: >-\n  old\n  more\n# end",
        "description:\nname:",
        "z: 1",
    ];
    let cases: Vec<(usize, String, String)> = (0..800)
        .map(|index| {
            let name = random();
            let description = random();
            (index, name, description)
        })
        .collect();
    for (index, name, description) in cases {
        let text = document_of(bases[index % bases.len()]);
        let result = edited(&text, &both(&name, &description));
        let fields = fields_of(&result);
        assert_eq!(
            fields.get("name"),
            Some(&Value::String(name.clone())),
            "{text:?} {name:?} {description:?}"
        );
        assert_eq!(
            fields.get("description"),
            Some(&Value::String(description.clone())),
            "{text:?} {name:?} {description:?}"
        );
        assert_eq!(body_of(&result), body_of(&text));
        assert_eq!(fields.get("z"), fields_of(&text).get("z"));
    }
}

#[test]
fn adding_missing_fields_adds_name_first_and_description_after_it() {
    assert_eq!(
        edited(&document_of("license: MIT"), &both("n", "d")),
        document_of("name: n\ndescription: d\nlicense: MIT")
    );
}

#[test]
fn adding_missing_fields_adds_only_the_missing_name() {
    assert_eq!(
        edited(&document_of("description: d\nz: 1"), &name("n")),
        document_of("name: n\ndescription: d\nz: 1")
    );
}

#[test]
fn adding_missing_fields_adds_the_missing_description_right_after_the_name() {
    assert_eq!(
        edited(&document_of("name: n # the name\nz: 1"), &description("d")),
        document_of("name: n # the name\ndescription: d\nz: 1")
    );
}

#[test]
fn adding_missing_fields_adds_the_description_after_a_multi_line_name_value() {
    assert_eq!(
        edited(&document_of("name: >\n  n\n  m\n\nz: 1"), &description("d")),
        document_of("name: >\n  n\n  m\ndescription: d\n\nz: 1")
    );
}

#[test]
fn adding_missing_fields_adds_the_description_when_the_name_is_the_last_key() {
    assert_eq!(
        edited(&document_of("name: n"), &description("d")),
        document_of("name: n\ndescription: d")
    );
    assert_eq!(
        edited(&document_of("name: n\n"), &description("d")),
        document_of("name: n\ndescription: d\n")
    );
}

#[test]
fn adding_missing_fields_adds_a_description_before_the_other_keys_when_there_is_no_name() {
    assert_eq!(
        edited(&document_of("z: 1"), &description("d")),
        document_of("description: d\nz: 1")
    );
}

#[test]
fn adding_missing_fields_adds_both_after_the_name_in_one_pass_when_only_one_is_missing() {
    assert_eq!(
        edited(&document_of("name: old\nz: 1"), &both("new", "d")),
        document_of("name: new\ndescription: d\nz: 1")
    );
}

#[test]
fn adding_missing_fields_keeps_the_comments_of_the_first_key() {
    assert_eq!(
        edited(&document_of("# about z\nz: 1"), &name("n")),
        document_of("name: n\n# about z\nz: 1")
    );
}

#[test]
fn adding_missing_fields_fills_an_empty_frontmatter() {
    assert_eq!(
        edited("---\n---\n\nBody.\n", &both("n", "d")),
        "---\nname: n\ndescription: d\n---\n\nBody.\n"
    );
    assert_eq!(edited("---\n\n---\n", &name("n")), "---\nname: n\n\n---\n");
}

#[test]
fn adding_missing_fields_fills_a_document_that_is_only_a_frontmatter() {
    assert_eq!(
        edited("---\nz: 1\n---", &name("n")),
        "---\nname: n\nz: 1\n---"
    );
}

#[test]
fn adding_missing_fields_adds_a_multi_line_description_as_a_literal_block() {
    assert_eq!(
        edited(&document_of("name: n\nz: 1"), &description("a\nb")),
        document_of("name: n\ndescription: |-\n  a\n  b\nz: 1")
    );
}

#[test]
fn adding_missing_fields_keeps_the_delimiters_blanks_included() {
    let text = "---  \nname: n\n---\t\n\nBody.\n";
    assert_eq!(
        edited(text, &both("m", "d")),
        "---  \nname: m\ndescription: d\n---\t\n\nBody.\n"
    );
}

#[test]
fn a_document_without_frontmatter_creates_one_at_the_top() {
    assert_eq!(
        edited("# Title\n\nBody.\n", &both("n", "d")),
        "---\nname: n\ndescription: d\n---\n\n# Title\n\nBody.\n"
    );
}

#[test]
fn a_document_without_frontmatter_keeps_the_whole_original_text_after_the_new_frontmatter() {
    let original = "\n\n  # Title\n\nBody.";
    let result = edited(original, &both("n", "d"));
    assert!(result.ends_with(original));
    assert_eq!(
        fields_of(&result),
        object(json!({"name": "n", "description": "d"}))
    );
}

#[test]
fn a_document_without_frontmatter_creates_one_for_an_empty_document() {
    assert_eq!(
        edited("", &both("n", "d")),
        "---\nname: n\ndescription: d\n---\n"
    );
}

#[test]
fn a_document_without_frontmatter_creates_only_the_fields_it_is_given() {
    assert_eq!(
        edited("Body.\n", &description("d")),
        "---\ndescription: d\n---\n\nBody.\n"
    );
}

#[test]
fn a_document_without_frontmatter_creates_a_frontmatter_above_a_rule_followed_by_prose() {
    let original = "---\n\nIntro.\n";
    let result = edited(original, &name("n"));
    assert_eq!(result, "---\nname: n\n---\n\n---\n\nIntro.\n");
    assert_eq!(fields_of(&result), object(json!({"name": "n"})));
    assert!(result.ends_with(original));
}

#[test]
fn a_document_without_frontmatter_creates_nothing_when_there_is_nothing_to_write() {
    assert_eq!(
        edited("# Title\n", &SkillFieldValues::default()),
        "# Title\n"
    );
}

#[test]
fn a_document_without_frontmatter_escapes_what_needs_it_in_a_new_frontmatter() {
    let result = edited("Body.\n", &both("n", "Does: a thing # really\nsecond line"));
    assert_eq!(
        fields_of(&result),
        object(json!({"name": "n", "description": "Does: a thing # really\nsecond line"}))
    );
}

#[test]
fn refusing_an_unclosed_frontmatter_creates_nothing() {
    let problem = set_skill_fields("---\nname: demo\n\n# Demo\n", &name("x")).unwrap_err();
    assert_eq!(problem.rule, FrontmatterProblemRule::FrontmatterNotClosed);
}

#[test]
fn refusing_a_frontmatter_with_a_syntax_error() {
    let problem = set_skill_fields(&document_of("name: [broken\nz: 1"), &name("x")).unwrap_err();
    assert_eq!(problem.rule, FrontmatterProblemRule::YamlSyntaxError);
}

#[test]
fn refusing_even_when_nothing_would_change() {
    assert!(set_skill_fields(&document_of("name: [broken"), &SkillFieldValues::default()).is_err());
}

#[test]
fn refusing_to_create_a_second_frontmatter_above_other_blocks() {
    for original in [
        "---\n{name: a, description: b}\n---\n# Demo\n",
        "---\n# name: a\n---\n# Demo\n",
        "---\n\nIntro.\n\n---\n\nMore.\n",
    ] {
        let problem = set_skill_fields(original, &name("x")).unwrap_err();
        assert_eq!(
            problem.rule,
            FrontmatterProblemRule::FrontmatterNotRecognized
        );
        assert_eq!(problem.message, "frontmatter not recognized");
        assert_eq!(problem.line, None);
    }
}

#[test]
fn refusing_a_document_starting_with_a_delimited_block_even_when_nothing_would_change() {
    assert!(
        set_skill_fields(
            "---\n{name: a}\n---\n# Demo\n",
            &SkillFieldValues::default()
        )
        .is_err()
    );
}

#[test]
fn refusing_a_frontmatter_with_a_duplicate_key() {
    let problem = set_skill_fields(&document_of("name: a\nname: b"), &name("x")).unwrap_err();
    assert_eq!(problem.rule, FrontmatterProblemRule::YamlSyntaxError);
}

#[test]
fn round_trip_parse_edit_parse_gives_the_new_values_and_the_same_body() {
    let text = document_of("# lead\nname: old\ndescription: >\n  Old\n  text\nlicense: MIT");
    let before = fields_of(&text);
    let result = edited(
        &text,
        &both(
            "new-name",
            "New: description # with specials\nand a second line",
        ),
    );
    let mut expected = before;
    expected.insert("name".into(), json!("new-name"));
    expected.insert(
        "description".into(),
        json!("New: description # with specials\nand a second line"),
    );
    assert_eq!(fields_of(&result), expected);
    assert_eq!(body_of(&result), body_of(&text));
}

#[test]
fn round_trip_editing_twice_equals_editing_once_with_the_last_values() {
    let text = document_of("name: a\ndescription: b");
    let twice = edited(&edited(&text, &name("x")), &name("y"));
    assert_eq!(twice, edited(&text, &name("y")));
}

#[test]
fn round_trip_is_stable_setting_the_values_it_just_wrote_changes_nothing() {
    let values = both("n", "one: two\nthree # four");
    let once = edited(&document_of("z: 1"), &values);
    assert_eq!(edited(&once, &values), once);
}

#[test]
fn round_trip_leaves_a_frontmatter_that_is_only_blank_lines_before_the_body_valid() {
    let result = edited("---\n\n\n---\n# Body\n", &name("n"));
    assert_eq!(fields_of(&result), object(json!({"name": "n"})));
    assert_eq!(body_of(&result), "\n# Body\n");
}

fn edits_of(text: &str, values: &SkillFieldValues) -> Vec<TextEdit> {
    match plan_skill_field_edits(text, values) {
        Ok(edits) => edits,
        Err(problem) => panic!("{}", problem.message),
    }
}

fn samples() -> Vec<(&'static str, String)> {
    vec![
        (
            "a document with frontmatter",
            document_of("name: a\ndescription: b\nlicense: MIT"),
        ),
        ("a frontmatter with only a name", document_of("name: a")),
        (
            "a frontmatter with only a description",
            document_of("description: b"),
        ),
        ("an empty frontmatter", "---\n---\n# Body\n".to_string()),
        (
            "a frontmatter with other keys",
            document_of("license: MIT\nmetadata:\n  a: b"),
        ),
        (
            "a block scalar",
            document_of("name: a\ndescription: |\n  one\n  two"),
        ),
        (
            "a quoted value",
            document_of("name: \"a\"\ndescription: 'b'"),
        ),
        ("no frontmatter", "# Title\n\nText.\n".to_string()),
        ("a body that starts with a dash", "- item\n".to_string()),
        (
            "a body that starts with a line break",
            "\n# Title\n".to_string(),
        ),
        ("an empty document", String::new()),
    ]
}

fn value_sets() -> Vec<SkillFieldValues> {
    vec![
        name("x"),
        description("Y."),
        both("x", "Y: z # w"),
        both("", "multi\nline"),
        both("123", "true"),
        SkillFieldValues::default(),
    ]
}

#[test]
fn plan_describes_the_same_text_as_set_skill_fields() {
    for (label, text) in samples() {
        for values in value_sets() {
            let text_after = edited(&text, &values);
            assert_eq!(
                apply_edits(&text, &edits_of(&text, &values)),
                text_after,
                "{label}"
            );
        }
    }
}

#[test]
fn plan_replaces_only_the_value_range_of_a_field_that_is_there() {
    let text = document_of("name: demo\ndescription: Does it.");
    let edits = edits_of(&text, &name("renamed"));
    assert_eq!(edits.len(), 1);
    assert_eq!(&text[edits[0].from..edits[0].to], ": demo\n");
    assert_eq!(edits[0].insert, ": renamed\n");
}

#[test]
fn plan_inserts_a_missing_field_and_touches_no_other_character() {
    let text = document_of("name: demo");
    let edits = edits_of(&text, &description("Does it."));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].from, edits[0].to);
    assert_eq!(edits[0].insert, "description: Does it.\n");
}

#[test]
fn plan_creates_the_frontmatter_with_one_insertion_at_the_start_whatever_the_body_begins_with() {
    for body in ["# Title\n", "- item\n", "---x\n", "name: y\n", "> quote\n"] {
        assert_eq!(
            edits_of(body, &name("x")),
            vec![TextEdit {
                from: 0,
                to: 0,
                insert: "---\nname: x\n---\n\n".to_string()
            }]
        );
    }
}

#[test]
fn plan_creates_no_separator_before_a_body_that_starts_with_a_line_break_or_is_empty() {
    for text in ["\n# Title\n", ""] {
        assert_eq!(
            edits_of(text, &name("x")),
            vec![TextEdit {
                from: 0,
                to: 0,
                insert: "---\nname: x\n---\n".to_string()
            }]
        );
    }
}

#[test]
fn plan_has_no_edit_when_nothing_changes() {
    assert_eq!(edits_of(&document_of("name: a"), &name("a")), vec![]);
    assert_eq!(edits_of("# Title\n", &SkillFieldValues::default()), vec![]);
}

#[test]
fn plan_keeps_every_edit_inside_the_frontmatter_when_there_is_one() {
    let text = document_of("name: a\ndescription: b");
    let frontmatter = find_frontmatter(&text).expect("frontmatter");
    for edit in edits_of(&text, &both("x", "y")) {
        assert!(edit.from >= frontmatter.yaml_from);
        let closing = text[frontmatter.yaml_from..]
            .find("\n---")
            .map(|index| index + frontmatter.yaml_from + 1)
            .expect("closing");
        assert!(edit.to <= closing);
    }
}

#[test]
fn plan_gives_edits_that_do_not_overlap_in_the_order_of_the_text() {
    let text = document_of("description: b\nname: a");
    let edits = edits_of(&text, &both("x", "y"));
    assert_eq!(edits.len(), 2);
    assert!(edits[0].to <= edits[1].from);
}

#[test]
fn plan_refuses_a_frontmatter_that_is_not_closed_not_recognized_or_has_a_yaml_error() {
    for (text, rule) in [
        (
            "---\nname: a\n\n# T\n",
            FrontmatterProblemRule::FrontmatterNotClosed,
        ),
        (
            "---\nprose here\n---\n",
            FrontmatterProblemRule::FrontmatterNotRecognized,
        ),
        (
            "---\nname: [a\n---\n",
            FrontmatterProblemRule::YamlSyntaxError,
        ),
    ] {
        let plan = plan_skill_field_edits(text, &name("x"));
        assert_eq!(plan.map_err(|problem| problem.rule), Err(rule), "{text:?}");
    }
}
