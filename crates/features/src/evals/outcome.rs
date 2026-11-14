use super::report::{EvalSummary, ReportOutcome, parse_eval_report, report_cost};
use crate::text::to_fixed;

#[derive(Clone, Debug)]
pub struct OutcomeInput {
    pub report_text: Option<String>,
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub cancelled: bool,
    pub max_cost_usd: f64,
    pub cli_version: Option<String>,
    pub limit_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunOutcome {
    Complete {
        summary: EvalSummary,
    },
    NotComplete {
        message: String,
        cost_usd: Option<f64>,
    },
}

const CANCELLED: &str = "Cancelled";

fn stop(message: impl Into<String>, cost_usd: Option<f64>) -> RunOutcome {
    RunOutcome::NotComplete {
        message: message.into(),
        cost_usd,
    }
}

fn limit_label(max_cost_usd: f64) -> String {
    let fixed = to_fixed(max_cost_usd, 4);
    let trailing = fixed
        .chars()
        .rev()
        .take(2)
        .take_while(|c| *c == '0')
        .count();
    fixed[..fixed.len() - trailing].to_string()
}

pub fn cost_limit_message(max_cost_usd: f64, note: Option<&str>) -> String {
    format!(
        "Stopped at the cost limit (US${}{})",
        limit_label(max_cost_usd),
        note.map_or(String::new(), |note| format!(", {note}"))
    )
}

fn exit_code_text(exit_code: Option<i32>) -> String {
    exit_code.map_or("null".to_string(), |code| code.to_string())
}

fn exit_message(exit_code: Option<i32>, signal: Option<&str>) -> String {
    match signal {
        Some(signal) => format!("Claude stopped with {signal}"),
        None => format!("Claude exited with code {}", exit_code_text(exit_code)),
    }
}

fn ended_by_cancel(exit_code: Option<i32>, signal: Option<&str>) -> bool {
    matches!(exit_code, Some(130 | 143)) || matches!(signal, Some("SIGTERM" | "SIGINT"))
}

fn no_report_message(exit_code: Option<i32>, signal: Option<&str>) -> String {
    if ended_by_cancel(exit_code, signal) {
        CANCELLED.to_string()
    } else if exit_code == Some(2) {
        "Claude exited with code 2 before it wrote a report".to_string()
    } else {
        exit_message(exit_code, signal)
    }
}

fn partial_message(
    reason: Option<&str>,
    exit_code: Option<i32>,
    signal: Option<&str>,
    max_cost_usd: f64,
    limit_note: Option<&str>,
) -> String {
    match reason {
        Some("interrupted") => return CANCELLED.to_string(),
        Some("cost_ceiling") => return cost_limit_message(max_cost_usd, limit_note),
        Some("auth_failed") => return "Claude is not logged in".to_string(),
        _ => {}
    }
    if ended_by_cancel(exit_code, signal) {
        return CANCELLED.to_string();
    }
    let early = format!(
        "Claude stopped early{}",
        reason.map_or(String::new(), |reason| format!(" ({reason})"))
    );
    match signal {
        Some(signal) => format!("{early} with {signal}"),
        None => format!("{early} with exit code {}", exit_code_text(exit_code)),
    }
}

pub fn map_run_outcome(input: &OutcomeInput) -> RunOutcome {
    let exit_code = input.exit_code;
    let signal = input.signal.as_deref();
    let cost_usd = input.report_text.as_deref().and_then(report_cost);
    if input.cancelled {
        return stop(CANCELLED, cost_usd);
    }
    let Some(report_text) = input.report_text.as_deref() else {
        return stop(no_report_message(exit_code, signal), None);
    };
    match parse_eval_report(report_text, input.cli_version.as_deref()) {
        ReportOutcome::Refused { message } => stop(message, cost_usd),
        ReportOutcome::Partial { reason, .. } => stop(
            partial_message(
                reason.as_deref(),
                exit_code,
                signal,
                input.max_cost_usd,
                input.limit_note.as_deref(),
            ),
            cost_usd,
        ),
        ReportOutcome::Complete { summary } => {
            if summary.cases.is_empty() {
                stop("No Evals in this skill", cost_usd)
            } else if !matches!(exit_code, Some(0 | 1)) {
                stop(exit_message(exit_code, signal), cost_usd)
            } else {
                RunOutcome::Complete { summary }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/data/evals/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture")
    }

    fn with_reason(reason: Option<&str>) -> String {
        let mut report: Value = serde_json::from_str(&fixture("auth-failed.json")).unwrap();
        report["partial"] = json!(true);
        report["partialReason"] = json!(reason);
        report.to_string()
    }

    fn no_cases() -> String {
        let mut report: Value = serde_json::from_str(&fixture("with-without.json")).unwrap();
        report["cases"] = json!([]);
        report.to_string()
    }

    fn input() -> OutcomeInput {
        OutcomeInput {
            report_text: Some(fixture("with-without.json")),
            exit_code: Some(0),
            signal: None,
            cancelled: false,
            max_cost_usd: 0.3,
            cli_version: Some("2.1.319".into()),
            limit_note: None,
        }
    }

    fn message(overrides: impl FnOnce(&mut OutcomeInput)) -> Option<String> {
        let mut base = input();
        overrides(&mut base);
        match map_run_outcome(&base) {
            RunOutcome::NotComplete { message, .. } => Some(message),
            RunOutcome::Complete { .. } => None,
        }
    }

    fn report(text: String) -> Option<String> {
        Some(text)
    }

    #[test]
    fn completes_on_exit_0_with_a_complete_report() {
        assert!(matches!(
            map_run_outcome(&input()),
            RunOutcome::Complete { .. }
        ));
    }

    #[test]
    fn completes_on_exit_1_with_a_complete_report() {
        let mut one = input();
        one.exit_code = Some(1);
        assert!(matches!(map_run_outcome(&one), RunOutcome::Complete { .. }));
    }

    #[test]
    fn says_cancelled_for_partial_reason_interrupted() {
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("interrupted")));
                i.exit_code = Some(130);
            })
            .as_deref(),
            Some("Cancelled")
        );
    }

    #[test]
    fn says_cancelled_for_exit_130_or_143_with_a_partial_report_of_another_reason() {
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(None));
                i.exit_code = Some(130);
            })
            .as_deref(),
            Some("Cancelled")
        );
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("x")));
                i.exit_code = Some(143);
            })
            .as_deref(),
            Some("Cancelled")
        );
    }

    #[test]
    fn says_cancelled_for_exit_130_or_143_without_a_report() {
        for code in [143, 130] {
            assert_eq!(
                message(|i| {
                    i.report_text = None;
                    i.exit_code = Some(code);
                })
                .as_deref(),
                Some("Cancelled")
            );
        }
    }

    #[test]
    fn says_cancelled_when_the_child_died_of_sigterm_or_sigint() {
        for signal in ["SIGTERM", "SIGINT"] {
            assert_eq!(
                message(|i| {
                    i.report_text = None;
                    i.exit_code = None;
                    i.signal = Some(signal.into());
                })
                .as_deref(),
                Some("Cancelled")
            );
        }
    }

    #[test]
    fn says_cancelled_for_a_user_cancel_over_every_other_signal() {
        assert_eq!(
            message(|i| i.cancelled = true).as_deref(),
            Some("Cancelled")
        );
        assert_eq!(
            message(|i| {
                i.cancelled = true;
                i.report_text = report(with_reason(Some("auth_failed")));
                i.exit_code = Some(2);
            })
            .as_deref(),
            Some("Cancelled")
        );
        assert_eq!(
            message(|i| {
                i.cancelled = true;
                i.report_text = None;
                i.exit_code = Some(2);
            })
            .as_deref(),
            Some("Cancelled")
        );
    }

    #[test]
    fn names_the_cost_limit() {
        let cost = with_reason(Some("cost_ceiling"));
        let said = |max: f64| {
            message(|i| {
                i.report_text = report(cost.clone());
                i.exit_code = Some(2);
                i.max_cost_usd = max;
            })
        };
        assert_eq!(
            said(0.3).as_deref(),
            Some("Stopped at the cost limit (US$0.30)")
        );
        assert_eq!(
            said(1.0).as_deref(),
            Some("Stopped at the cost limit (US$1.00)")
        );
        assert_eq!(
            said(0.125).as_deref(),
            Some("Stopped at the cost limit (US$0.125)")
        );
    }

    #[test]
    fn keeps_the_users_limit_visible_and_adds_a_note_for_a_shared_limit() {
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("cost_ceiling")));
                i.exit_code = Some(2);
                i.max_cost_usd = 1.0;
                i.limit_note = Some("half for each side".into());
            })
            .as_deref(),
            Some("Stopped at the cost limit (US$1.00, half for each side)")
        );
    }

    #[test]
    fn says_claude_is_not_logged_in() {
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("auth_failed")));
                i.exit_code = Some(2);
            })
            .as_deref(),
            Some("Claude is not logged in")
        );
    }

    #[test]
    fn lets_partial_reason_win_over_the_exit_code() {
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("auth_failed")));
                i.exit_code = Some(130);
            })
            .as_deref(),
            Some("Claude is not logged in")
        );
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("cost_ceiling")));
                i.exit_code = Some(1);
            })
            .as_deref(),
            Some("Stopped at the cost limit (US$0.30)")
        );
    }

    #[test]
    fn does_not_guess_the_reason_for_exit_2_without_a_report() {
        assert_eq!(
            message(|i| {
                i.report_text = None;
                i.exit_code = Some(2);
            })
            .as_deref(),
            Some("Claude exited with code 2 before it wrote a report")
        );
    }

    #[test]
    fn does_not_guess_for_a_partial_report_of_unknown_reason() {
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("new_reason")));
                i.exit_code = Some(2);
            })
            .as_deref(),
            Some("Claude stopped early (new_reason) with exit code 2")
        );
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(None));
                i.exit_code = Some(2);
            })
            .as_deref(),
            Some("Claude stopped early with exit code 2")
        );
        assert_eq!(
            message(|i| {
                i.report_text = report(with_reason(Some("new_reason")));
                i.exit_code = None;
                i.signal = Some("SIGKILL".into());
            })
            .as_deref(),
            Some("Claude stopped early (new_reason) with SIGKILL")
        );
    }

    #[test]
    fn gives_the_adapter_message_with_the_cli_version_for_another_schema_version() {
        let mut report_value: Value = serde_json::from_str(&fixture("with-without.json")).unwrap();
        report_value["schemaVersion"] = json!(2);
        assert_eq!(
            message(|i| i.report_text = Some(report_value.to_string())).as_deref(),
            Some(
                "The Eval report has schemaVersion 2, which this app does not read (Claude CLI 2.1.319)"
            )
        );
    }

    #[test]
    fn gives_the_adapter_message_for_unparsable_json() {
        assert_eq!(
            message(|i| i.report_text = report("not json".into())).as_deref(),
            Some("The Eval report could not be read (Claude CLI 2.1.319)")
        );
    }

    #[test]
    fn says_an_unknown_version_only_when_the_cli_version_is_unknown_too() {
        assert_eq!(
            message(|i| {
                i.report_text = report("not json".into());
                i.cli_version = None;
            })
            .as_deref(),
            Some("The Eval report could not be read (an unknown Claude CLI version)")
        );
    }

    #[test]
    fn says_no_evals_in_this_skill_for_no_cases_whatever_the_exit_code() {
        for code in [0, 1, 2] {
            assert_eq!(
                message(|i| {
                    i.report_text = report(no_cases());
                    i.exit_code = Some(code);
                })
                .as_deref(),
                Some("No Evals in this skill")
            );
        }
    }

    #[test]
    fn does_not_call_a_complete_report_with_an_unexpected_exit_code_complete() {
        assert_eq!(
            message(|i| i.exit_code = Some(3)).as_deref(),
            Some("Claude exited with code 3")
        );
    }

    #[test]
    fn names_the_signal_when_there_is_no_report_and_no_exit_code() {
        assert_eq!(
            message(|i| {
                i.report_text = None;
                i.exit_code = None;
                i.signal = Some("SIGKILL".into());
            })
            .as_deref(),
            Some("Claude stopped with SIGKILL")
        );
    }

    #[test]
    fn never_carries_a_summary_for_a_cancelled_run() {
        let mut cancelled = input();
        cancelled.report_text = report(with_reason(Some("interrupted")));
        cancelled.exit_code = Some(130);
        assert_eq!(
            map_run_outcome(&cancelled),
            RunOutcome::NotComplete {
                message: "Cancelled".into(),
                cost_usd: Some(0.0)
            }
        );
    }

    #[test]
    fn keeps_the_cost_of_a_stopped_run_from_any_report_that_parses() {
        let cost = |text: Option<String>, change: &dyn Fn(&mut OutcomeInput)| {
            let mut base = input();
            base.report_text = text;
            change(&mut base);
            match map_run_outcome(&base) {
                RunOutcome::NotComplete { cost_usd, .. } => Some(cost_usd),
                RunOutcome::Complete { .. } => None,
            }
        };
        assert_eq!(
            cost(report(with_reason(Some("cost_ceiling"))), &|i| i
                .exit_code =
                Some(2)),
            Some(Some(0.0))
        );
        let no_cases_cost = serde_json::from_str::<Value>(&no_cases()).unwrap()["costUsd"].as_f64();
        assert_eq!(
            cost(report(no_cases()), &|i| i.exit_code = Some(1)),
            Some(no_cases_cost)
        );
        assert_eq!(
            cost(report(with_reason(Some("interrupted"))), &|i| i.cancelled =
                true),
            Some(Some(0.0))
        );
        assert_eq!(cost(None, &|i| i.exit_code = Some(143)), Some(None));
        assert_eq!(cost(report("not json".into()), &|_| {}), Some(None));
        let mut priced: Value = serde_json::from_str(&with_reason(Some("interrupted"))).unwrap();
        priced["costUsd"] = json!(0.0015);
        assert_eq!(
            cost(report(priced.to_string()), &|_| {}),
            Some(Some(0.0015))
        );
    }
}
