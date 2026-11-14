use std::collections::HashMap;

use super::baseline::{SummaryBaseline, parse_summary_baseline};
use super::report::{CaseRuns, CaseTally, EvalSummary, ReportOutcome, parse_eval_report};
use super::tally::{Tally, sum_tallies};

pub type VersionBaseline = SummaryBaseline;

const EMPTY: Tally = Tally {
    passed: 0,
    total: 0,
};

fn case_of(name: &str, today: Option<&CaseTally>, older: Option<&CaseTally>) -> CaseTally {
    CaseTally {
        name: name.to_string(),
        with: today.map_or(EMPTY, |case| case.with),
        without: older.map(|case| case.with),
        runs: CaseRuns {
            with: today.map(|case| case.runs.with.clone()).unwrap_or_default(),
            without: older.map(|case| case.runs.with.clone()),
        },
    }
}

pub fn compare_summaries(
    today: &EvalSummary,
    older: &EvalSummary,
    baseline: VersionBaseline,
) -> EvalSummary {
    let today_by_name: HashMap<&str, &CaseTally> = today
        .cases
        .iter()
        .map(|case| (case.name.as_str(), case))
        .collect();
    let older_by_name: HashMap<&str, &CaseTally> = older
        .cases
        .iter()
        .map(|case| (case.name.as_str(), case))
        .collect();
    let mut names: Vec<&str> = Vec::new();
    for case in today.cases.iter().chain(&older.cases) {
        if !names.contains(&case.name.as_str()) {
            names.push(case.name.as_str());
        }
    }
    let cases: Vec<CaseTally> = names
        .iter()
        .map(|name| {
            case_of(
                name,
                today_by_name.get(name).copied(),
                older_by_name.get(name).copied(),
            )
        })
        .collect();
    let older_tallies: Vec<Tally> = cases.iter().filter_map(|case| case.without).collect();
    EvalSummary {
        claude_version: today.claude_version.clone(),
        started_at: today.started_at.clone(),
        cost_usd: today.cost_usd + older.cost_usd,
        with: sum_tallies(cases.iter().map(|case| case.with)),
        without: (!older_tallies.is_empty()).then(|| sum_tallies(older_tallies)),
        cases,
        baseline,
    }
}

pub fn compare_reports(
    today_text: &str,
    older_text: &str,
    baseline: VersionBaseline,
    cli_version: Option<&str>,
) -> ReportOutcome {
    let ReportOutcome::Complete { summary: today } = parse_eval_report(today_text, cli_version)
    else {
        return parse_eval_report(today_text, cli_version);
    };
    let ReportOutcome::Complete { summary: older } = parse_eval_report(older_text, cli_version)
    else {
        return parse_eval_report(older_text, cli_version);
    };
    ReportOutcome::Complete {
        summary: compare_summaries(&today, &older, baseline),
    }
}

#[derive(Clone, Debug)]
pub struct StoredRun {
    pub baseline_text: Option<String>,
    pub first_text: String,
    pub second_text: Option<String>,
}

pub fn summary_of_stored(stored: &StoredRun) -> Option<EvalSummary> {
    let baseline = match &stored.baseline_text {
        None => SummaryBaseline::None,
        Some(text) => parse_summary_baseline(text)?,
    };
    if baseline == SummaryBaseline::None {
        return match parse_eval_report(&stored.first_text, None) {
            ReportOutcome::Complete { summary } => Some(summary),
            _ => None,
        };
    }
    let second = stored.second_text.as_deref()?;
    match compare_reports(&stored.first_text, second, baseline, None) {
        ReportOutcome::Complete { summary } => Some(summary),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::evals::tally::{RunRecord, arm_tally};

    fn run(passed: bool, fired: Option<bool>) -> RunRecord {
        RunRecord {
            passed,
            fired,
            errored: false,
        }
    }

    fn case_named(name: &str, runs: Vec<RunRecord>) -> CaseTally {
        CaseTally {
            name: name.into(),
            with: arm_tally(&runs),
            without: None,
            runs: CaseRuns {
                with: runs,
                without: None,
            },
        }
    }

    fn summary_of(cases: Vec<CaseTally>, cost_usd: f64) -> EvalSummary {
        EvalSummary {
            claude_version: "2.1.319".into(),
            started_at: "2026-11-07T20:00:00.000Z".into(),
            cost_usd,
            with: sum_tallies(cases.iter().map(|case| case.with)),
            without: None,
            cases,
            baseline: SummaryBaseline::None,
        }
    }

    const BASELINE: SummaryBaseline = SummaryBaseline::Version {
        version_id: 7,
        at: 123,
    };

    fn tally(passed: usize, total: usize) -> Tally {
        Tally { passed, total }
    }

    #[test]
    fn puts_todays_passes_on_one_side_and_the_versions_on_the_other_per_eval() {
        let today = summary_of(
            vec![
                case_named("a", vec![run(true, Some(true)), run(true, Some(true))]),
                case_named("b", vec![run(false, None), run(true, None)]),
            ],
            0.01,
        );
        let older = summary_of(
            vec![
                case_named("a", vec![run(true, Some(false)), run(false, Some(false))]),
                case_named("b", vec![run(false, None), run(false, None)]),
            ],
            0.01,
        );
        let result = compare_summaries(&today, &older, BASELINE);
        let rows: Vec<_> = result
            .cases
            .iter()
            .map(|c| (c.name.as_str(), c.with, c.without))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("a", tally(2, 2), Some(tally(1, 2))),
                ("b", tally(1, 2), Some(tally(0, 2)))
            ]
        );
        assert_eq!(result.with, tally(3, 4));
        assert_eq!(result.without, Some(tally(1, 4)));
        assert_eq!(result.baseline, BASELINE);
    }

    #[test]
    fn keeps_the_fired_records_of_the_version_side() {
        let result = compare_summaries(
            &summary_of(vec![case_named("a", vec![run(true, Some(true))])], 0.01),
            &summary_of(vec![case_named("a", vec![run(true, Some(false))])], 0.01),
            BASELINE,
        );
        assert_eq!(
            result.cases[0].runs.without,
            Some(vec![run(true, Some(false))])
        );
    }

    #[test]
    fn sums_the_cost_of_both_runs_and_keeps_the_first_run_for_the_rest() {
        let result = compare_summaries(
            &summary_of(vec![case_named("a", vec![run(true, None)])], 0.01),
            &summary_of(vec![case_named("a", vec![run(true, None)])], 0.02),
            BASELINE,
        );
        assert!((result.cost_usd - 0.03).abs() < 1e-9);
        assert_eq!(result.started_at, "2026-11-07T20:00:00.000Z");
    }

    #[test]
    fn leaves_an_eval_that_only_today_has_with_no_side_for_the_version() {
        let result = compare_summaries(
            &summary_of(
                vec![
                    case_named("a", vec![run(true, None)]),
                    case_named("only-today", vec![run(true, None)]),
                ],
                0.01,
            ),
            &summary_of(vec![case_named("a", vec![run(false, None)])], 0.01),
            BASELINE,
        );
        let extra = result
            .cases
            .iter()
            .find(|c| c.name == "only-today")
            .unwrap();
        assert_eq!(extra.without, None);
        assert_eq!(extra.runs.without, None);
        assert_eq!(extra.with, tally(1, 1));
        assert_eq!(result.without, Some(tally(0, 1)));
    }

    #[test]
    fn keeps_an_eval_that_only_the_version_has_with_an_empty_side_for_today() {
        let result = compare_summaries(
            &summary_of(vec![case_named("a", vec![run(true, None)])], 0.01),
            &summary_of(
                vec![
                    case_named("a", vec![run(true, None)]),
                    case_named("only-older", vec![run(true, None)]),
                ],
                0.01,
            ),
            BASELINE,
        );
        let extra = result
            .cases
            .iter()
            .find(|c| c.name == "only-older")
            .unwrap();
        assert_eq!(extra.with, tally(0, 0));
        assert_eq!(extra.runs.with, vec![]);
        assert_eq!(extra.without, Some(tally(1, 1)));
    }

    fn report(partial: bool) -> String {
        json!({
            "schemaVersion": 1,
            "claudeVersion": "2.1.319",
            "startedAt": "2026-11-07T20:00:00.000Z",
            "costUsd": 0.01,
            "partial": partial,
            "partialReason": if partial { json!("interrupted") } else { json!(null) },
            "cases": [{"name": "a", "graders": [], "arms": {"with": [{"passed": true, "graders": []}]}}]
        })
        .to_string()
    }

    #[test]
    fn is_complete_when_both_reports_are() {
        let outcome = compare_reports(&report(false), &report(false), BASELINE, None);
        assert!(matches!(outcome, ReportOutcome::Complete { .. }));
        assert!(matches!(
            parse_eval_report(&report(false), None),
            ReportOutcome::Complete { .. }
        ));
    }

    #[test]
    fn is_not_complete_when_either_report_is_partial() {
        assert!(matches!(
            compare_reports(&report(true), &report(false), BASELINE, None),
            ReportOutcome::Partial { .. }
        ));
        assert!(matches!(
            compare_reports(&report(false), &report(true), BASELINE, None),
            ReportOutcome::Partial { .. }
        ));
    }

    #[test]
    fn refuses_when_either_report_cannot_be_read() {
        assert!(matches!(
            compare_reports("nope", &report(false), BASELINE, None),
            ReportOutcome::Refused { .. }
        ));
        assert!(matches!(
            compare_reports(&report(false), "nope", BASELINE, None),
            ReportOutcome::Refused { .. }
        ));
    }

    #[test]
    fn reads_a_run_with_no_baseline_file_as_a_run_against_no_skill() {
        let summary = summary_of_stored(&StoredRun {
            baseline_text: None,
            first_text: report(false),
            second_text: None,
        });
        assert_eq!(summary.map(|s| s.baseline), Some(SummaryBaseline::None));
    }

    #[test]
    fn reads_a_version_run_from_both_reports_and_the_baseline_file() {
        let summary = summary_of_stored(&StoredRun {
            baseline_text: Some(json!({"kind": "version", "versionId": 7, "at": 123}).to_string()),
            first_text: report(false),
            second_text: Some(report(false)),
        });
        assert_eq!(summary.map(|s| s.baseline), Some(BASELINE));
    }

    #[test]
    fn shows_nothing_when_the_baseline_file_or_the_second_report_is_missing_or_broken() {
        let version = json!({"kind": "version", "versionId": 7, "at": 123}).to_string();
        assert_eq!(
            summary_of_stored(&StoredRun {
                baseline_text: Some("x".into()),
                first_text: report(false),
                second_text: None
            }),
            None
        );
        assert_eq!(
            summary_of_stored(&StoredRun {
                baseline_text: Some(version.clone()),
                first_text: report(false),
                second_text: None
            }),
            None
        );
        assert_eq!(
            summary_of_stored(&StoredRun {
                baseline_text: Some(version),
                first_text: report(false),
                second_text: Some(report(true))
            }),
            None
        );
    }
}
