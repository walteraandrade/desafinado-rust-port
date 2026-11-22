use std::collections::HashSet;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::baseline::{NO_BASELINE, SummaryBaseline};
use super::tally::{RunRecord, Tally, arm_tally, sum_tallies};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseRuns {
    pub with: Vec<RunRecord>,
    pub without: Option<Vec<RunRecord>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseTally {
    pub name: String,
    pub with: Tally,
    pub without: Option<Tally>,
    pub runs: CaseRuns,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalSummary {
    pub claude_version: String,
    pub started_at: String,
    pub cost_usd: f64,
    pub cases: Vec<CaseTally>,
    pub with: Tally,
    pub without: Option<Tally>,
    pub baseline: SummaryBaseline,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReportOutcome {
    Complete {
        summary: EvalSummary,
    },
    Partial {
        reason: Option<String>,
        claude_version: String,
    },
    Refused {
        message: String,
    },
}

const SUPPORTED_SCHEMA_VERSION: f64 = 1.0;

#[derive(Deserialize)]
struct GraderResult {
    name: String,
    passed: bool,
}

#[derive(Deserialize)]
struct Entry {
    passed: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    graders: Vec<GraderResult>,
}

#[derive(Deserialize)]
struct GraderDefinition {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    config: Option<Map<String, Value>>,
}

#[derive(Deserialize)]
struct Arms {
    with: Vec<Entry>,
    #[serde(default)]
    without: Option<Vec<Entry>>,
}

#[derive(Deserialize)]
struct ReportCase {
    name: String,
    #[serde(default)]
    graders: Vec<GraderDefinition>,
    arms: Arms,
}

#[derive(Deserialize)]
struct Plugin {
    name: String,
}

#[derive(Deserialize)]
struct Suite {
    #[serde(default)]
    plugins: Vec<Plugin>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    claude_version: String,
    started_at: String,
    cost_usd: f64,
    partial: bool,
    #[serde(default)]
    partial_reason: Option<String>,
    #[serde(default)]
    suite: Option<Suite>,
    cases: Vec<ReportCase>,
}

pub fn matches_skill_pattern(pattern: &str, skill_name: &str) -> bool {
    let Ok(regex) = Regex::new(pattern) else {
        return false;
    };
    [skill_name.to_string(), format!("{skill_name}:{skill_name}")]
        .iter()
        .any(|name| regex.is_match(&serde_json::json!({ "skill": name }).to_string()))
}

pub fn matches_skill_name(pattern: Option<&Value>, skill_name: &str) -> bool {
    match pattern.and_then(Value::as_str) {
        Some(text) => matches_skill_pattern(text, skill_name),
        None => true,
    }
}

fn is_fired_indicator(definition: &GraderDefinition, skill_name: Option<&str>) -> bool {
    let Some(config) = &definition.config else {
        return false;
    };
    definition.kind == "tool_used"
        && config.get("tool").and_then(Value::as_str) == Some("Skill")
        && config.get("max").and_then(Value::as_f64) != Some(0.0)
        && skill_name.is_none_or(|name| matches_skill_name(config.get("input_match"), name))
}

fn ended_without_grading(entry: &Entry) -> bool {
    entry.error.is_some() && entry.graders.is_empty()
}

fn record_of(entries: &[Entry], indicators: Option<&HashSet<&str>>) -> Vec<RunRecord> {
    entries
        .iter()
        .map(|item| RunRecord {
            passed: item.passed,
            errored: ended_without_grading(item),
            fired: match indicators {
                Some(names) if !names.is_empty() => Some(
                    item.graders
                        .iter()
                        .any(|grader| grader.passed && names.contains(grader.name.as_str())),
                ),
                _ => None,
            },
        })
        .collect()
}

fn version_label(claude_version: Option<&str>) -> String {
    match claude_version {
        None => "an unknown Claude CLI version".to_string(),
        Some(version) => format!("Claude CLI {version}"),
    }
}

fn refuse(claude_version: Option<&str>, what: &str) -> ReportOutcome {
    ReportOutcome::Refused {
        message: format!("{what} ({})", version_label(claude_version)),
    }
}

fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => match number.as_f64() {
            Some(float) if float.fract() == 0.0 && float.abs() < 1e15 => (float as i64).to_string(),
            _ => number.to_string(),
        },
        Some(Value::Null) => "null".to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Array(_)) => String::new(),
        Some(Value::Object(_)) => "[object Object]".to_string(),
    }
}

pub fn report_cost(text: &str) -> Option<f64> {
    let parsed: Value = serde_json::from_str(text).ok()?;
    parsed.as_object()?.get("costUsd")?.as_f64()
}

pub fn parse_eval_report(text: &str, cli_version: Option<&str>) -> ReportOutcome {
    let parsed: Option<Value> = serde_json::from_str(text).ok();
    let Some(Value::Object(envelope)) = &parsed else {
        return refuse(cli_version, "The Eval report could not be read");
    };
    let envelope_version = match envelope.get("claudeVersion") {
        None => None,
        Some(Value::String(version)) => Some(version.as_str()),
        Some(_) => return refuse(cli_version, "The Eval report could not be read"),
    };
    let claude_version = envelope_version.or(cli_version);
    let schema_version = envelope.get("schemaVersion");
    if schema_version.and_then(Value::as_f64) != Some(SUPPORTED_SCHEMA_VERSION) {
        return refuse(
            claude_version,
            &format!(
                "The Eval report has schemaVersion {}, which this app does not read",
                js_string(schema_version)
            ),
        );
    }
    let Some(report) = parsed
        .clone()
        .and_then(|value| serde_json::from_value::<Report>(value).ok())
    else {
        return refuse(claude_version, "The Eval report has an unexpected shape");
    };
    if report.partial {
        return ReportOutcome::Partial {
            reason: report.partial_reason,
            claude_version: report.claude_version,
        };
    }
    let skill_name = report
        .suite
        .as_ref()
        .and_then(|suite| suite.plugins.first())
        .map(|plugin| plugin.name.as_str());
    let cases: Vec<CaseTally> = report
        .cases
        .iter()
        .map(|case| {
            let indicators: HashSet<&str> = case
                .graders
                .iter()
                .filter(|grader| is_fired_indicator(grader, skill_name))
                .map(|grader| grader.name.as_str())
                .collect();
            let with_runs = record_of(&case.arms.with, Some(&indicators));
            let without_runs = case
                .arms
                .without
                .as_ref()
                .map(|entries| record_of(entries, None));
            CaseTally {
                name: case.name.clone(),
                with: arm_tally(&with_runs),
                without: without_runs.as_deref().map(arm_tally),
                runs: CaseRuns {
                    with: with_runs,
                    without: without_runs,
                },
            }
        })
        .collect();
    let without_tallies: Vec<Tally> = cases.iter().filter_map(|case| case.without).collect();
    ReportOutcome::Complete {
        summary: EvalSummary {
            claude_version: report.claude_version,
            started_at: report.started_at,
            cost_usd: report.cost_usd,
            with: sum_tallies(cases.iter().map(|case| case.with)),
            without: (!without_tallies.is_empty()).then(|| sum_tallies(without_tallies)),
            cases,
            baseline: NO_BASELINE,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    pub fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/data/evals/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture")
    }

    fn edited(edit: impl FnOnce(&mut Value)) -> String {
        let mut report: Value = serde_json::from_str(&fixture("with-without.json")).unwrap();
        edit(&mut report);
        report.to_string()
    }

    fn complete(outcome: ReportOutcome) -> EvalSummary {
        match outcome {
            ReportOutcome::Complete { summary } => summary,
            other => panic!("expected complete, got {other:?}"),
        }
    }

    fn record(passed: bool, fired: Option<bool>) -> RunRecord {
        RunRecord {
            passed,
            fired,
            errored: false,
        }
    }

    #[test]
    fn counts_passes_per_arm_from_the_entries_of_a_complete_run() {
        let outcome = parse_eval_report(&fixture("with-without.json"), None);
        assert_eq!(
            outcome,
            ReportOutcome::Complete {
                summary: EvalSummary {
                    claude_version: "2.1.319".into(),
                    started_at: "2026-11-07T20:03:24.309Z".into(),
                    cost_usd: 0.002_654_97,
                    cases: vec![CaseTally {
                        name: "greet".into(),
                        with: Tally {
                            passed: 1,
                            total: 1
                        },
                        without: Some(Tally {
                            passed: 0,
                            total: 1
                        }),
                        runs: CaseRuns {
                            with: vec![record(true, None)],
                            without: Some(vec![record(false, None)]),
                        },
                    }],
                    with: Tally {
                        passed: 1,
                        total: 1
                    },
                    without: Some(Tally {
                        passed: 0,
                        total: 1
                    }),
                    baseline: SummaryBaseline::None,
                }
            }
        );
    }

    #[test]
    fn counts_entries_not_runs_per_case() {
        let text = edited(|report| report["cases"][0]["runsPerCase"] = json!(3));
        assert_eq!(complete(parse_eval_report(&text, None)).with.total, 1);
    }

    #[test]
    fn sums_several_cases_and_runs() {
        let text = edited(|report| {
            let first = report["cases"][0].clone();
            let mut failing = first["arms"]["with"][0].clone();
            failing["passed"] = json!(false);
            let arms = report["cases"][0]["arms"]["with"].as_array_mut().unwrap();
            arms.push(failing);
            arms.push(first["arms"]["with"][0].clone());
            let mut second = report["cases"][0].clone();
            second["name"] = json!("second");
            report["cases"].as_array_mut().unwrap().push(second);
        });
        let summary = complete(parse_eval_report(&text, None));
        assert_eq!(
            summary.with,
            Tally {
                passed: 4,
                total: 6
            }
        );
        assert_eq!(
            summary.cases[0].with,
            Tally {
                passed: 2,
                total: 3
            }
        );
    }

    #[test]
    fn records_per_run_whether_a_tool_used_skill_grader_passed() {
        let text = edited(|report| {
            report["cases"][0]["graders"].as_array_mut().unwrap().push(
                json!({"name": "skill-fired", "type": "tool_used", "config": {"tool": "Skill"}}),
            );
            let run = &mut report["cases"][0]["arms"]["with"][0];
            run["graders"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name": "skill-fired", "passed": true}));
            let mut bare = run.clone();
            bare["graders"] = json!([{"name": "skill-fired", "passed": false}]);
            report["cases"][0]["arms"]["with"]
                .as_array_mut()
                .unwrap()
                .push(bare);
        });
        let summary = complete(parse_eval_report(&text, None));
        assert_eq!(
            summary.cases[0].runs.with,
            vec![record(true, Some(true)), record(true, Some(false))]
        );
        assert_eq!(
            summary.cases[0].runs.without,
            Some(vec![record(false, None)])
        );
    }

    #[test]
    fn does_not_count_a_tool_used_grader_on_another_tool_as_firing() {
        let text = edited(|report| {
            report["cases"][0]["graders"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name": "read", "type": "tool_used", "config": {"tool": "Read"}}));
        });
        let summary = complete(parse_eval_report(&text, None));
        assert_eq!(summary.cases[0].runs.with, vec![record(true, None)]);
    }

    #[test]
    fn has_no_without_arm_when_the_run_had_a_single_one() {
        let text = edited(|report| {
            report["cases"][0]["arms"]
                .as_object_mut()
                .unwrap()
                .remove("without");
        });
        assert_eq!(complete(parse_eval_report(&text, None)).without, None);
    }

    #[test]
    fn ignores_fields_it_does_not_know() {
        let text = edited(|report| {
            report["somethingNew"] = json!({"a": 1});
            report["cases"][0]["arms"]["with"][0]["extra"] = json!(true);
        });
        assert!(matches!(
            parse_eval_report(&text, None),
            ReportOutcome::Complete { .. }
        ));
    }

    #[test]
    fn shows_no_pass_for_a_partial_run() {
        assert_eq!(
            parse_eval_report(&fixture("auth-failed.json"), None),
            ReportOutcome::Partial {
                reason: Some("auth_failed".into()),
                claude_version: "2.1.319".into()
            }
        );
    }

    #[test]
    fn refuses_another_schema_version_naming_the_cli_version() {
        let text = edited(|report| report["schemaVersion"] = json!(2));
        match parse_eval_report(&text, None) {
            ReportOutcome::Refused { message } => assert!(message.contains("2.1.319")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn refuses_a_report_that_does_not_parse() {
        assert!(matches!(
            parse_eval_report("{not json", None),
            ReportOutcome::Refused { .. }
        ));
        assert!(matches!(
            parse_eval_report("[]", None),
            ReportOutcome::Refused { .. }
        ));
    }

    #[test]
    fn refuses_a_version_1_report_with_the_wrong_shape() {
        let text = edited(|report| report["cases"] = json!("none"));
        match parse_eval_report(&text, None) {
            ReportOutcome::Refused { message } => assert!(message.contains("2.1.319")),
            other => panic!("{other:?}"),
        }
    }

    #[expect(
        clippy::option_option,
        reason = "mirrors fired: None when the run has no report"
    )]
    fn fired_with(grader: Value, skill_name: &str) -> Option<Option<bool>> {
        let text = edited(|report| {
            report["suite"]["plugins"] = json!([{ "name": skill_name }]);
            let mut definition = json!({"name": "indicator", "type": "tool_used"});
            for (key, value) in grader.as_object().unwrap() {
                definition[key] = value.clone();
            }
            report["cases"][0]["graders"]
                .as_array_mut()
                .unwrap()
                .push(definition);
            report["cases"][0]["arms"]["with"][0]["graders"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name": "indicator", "passed": true}));
        });
        match parse_eval_report(&text, None) {
            ReportOutcome::Complete { summary } => Some(summary.cases[0].runs.with[0].fired),
            _ => None,
        }
    }

    #[test]
    fn ignores_a_must_not_invoke_check() {
        assert_eq!(
            fired_with(
                json!({"config": {"tool": "Skill", "min": 0, "max": 0}}),
                "fx-skill"
            ),
            Some(None)
        );
    }

    #[test]
    fn needs_input_match_to_name_this_skill_with_or_without_a_plugin_prefix() {
        let pattern = r#""skill"\s*:\s*"(?:[\w-]+:)?fx-skill""#;
        assert_eq!(
            fired_with(
                json!({"config": {"tool": "Skill", "input_match": pattern}}),
                "fx-skill"
            ),
            Some(Some(true))
        );
        assert_eq!(
            fired_with(
                json!({"config": {"tool": "Skill", "input_match": "\"skill\":\"fx-skill:fx-skill\""}}),
                "fx-skill"
            ),
            Some(Some(true))
        );
    }

    #[test]
    fn ignores_a_skill_grader_that_matches_another_skill() {
        assert_eq!(
            fired_with(
                json!({"config": {"tool": "Skill", "input_match": r#""skill"\s*:\s*"other""#}}),
                "fx-skill"
            ),
            Some(None)
        );
    }

    fn first_run(edit: impl FnOnce(&mut Value)) -> CaseTally {
        let text = edited(|report| edit(&mut report["cases"][0]["arms"]["with"][0]));
        complete(parse_eval_report(&text, None)).cases.remove(0)
    }

    #[test]
    fn leaves_out_a_run_that_scored_0_without_grading() {
        let item = first_run(|entry| {
            entry["error"] = json!("API Error: 429");
            entry["passed"] = json!(false);
            entry["graders"] = json!([]);
        });
        assert_eq!(
            item.with,
            Tally {
                passed: 0,
                total: 0
            }
        );
        assert!(item.runs.with[0].errored);
    }

    #[test]
    fn keeps_a_run_with_an_error_that_was_still_graded() {
        let item = first_run(|entry| entry["error"] = json!("timed out after 300s"));
        assert_eq!(
            item.with,
            Tally {
                passed: 1,
                total: 1
            }
        );
        assert!(!item.runs.with[0].errored);
    }

    #[test]
    fn reads_the_cost_of_any_report_that_parses() {
        assert_eq!(report_cost("{\"costUsd\": 0.5}"), Some(0.5));
        assert_eq!(report_cost("nope"), None);
        assert_eq!(report_cost("{}"), None);
    }
}
