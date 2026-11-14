use super::types::EvalRunResult;
use super::view::stopped_text;

pub fn failure_reason(result: &EvalRunResult) -> String {
    match result {
        EvalRunResult::Failed { reason } => reason.clone(),
        EvalRunResult::NoCli => "The Claude CLI was not found".to_string(),
        EvalRunResult::RisksChanged => {
            "The Skill changed while you were confirming. Run again.".to_string()
        }
        EvalRunResult::Busy => "Another Eval run is in progress".to_string(),
        EvalRunResult::Completed { .. } | EvalRunResult::NotComplete { .. } => String::new(),
    }
}

pub fn ended_text(name: &str, result: &EvalRunResult) -> String {
    match result {
        EvalRunResult::Completed { .. } => format!("Evals for {name} finished"),
        EvalRunResult::NotComplete { message, cost_usd } => {
            format!("{name}: {}", stopped_text(message, *cost_usd))
        }
        other => format!("{name}: {}", failure_reason(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evals::baseline::SummaryBaseline;
    use crate::evals::report::EvalSummary;

    fn summary() -> EvalSummary {
        EvalSummary {
            claude_version: String::new(),
            started_at: String::new(),
            cost_usd: 0.0,
            cases: vec![],
            with: crate::evals::tally::Tally {
                passed: 0,
                total: 0,
            },
            without: None,
            baseline: SummaryBaseline::None,
        }
    }

    #[test]
    fn explains_each_run_result_that_is_not_a_summary() {
        assert_eq!(
            failure_reason(&EvalRunResult::Busy),
            "Another Eval run is in progress"
        );
        assert_eq!(
            failure_reason(&EvalRunResult::NoCli),
            "The Claude CLI was not found"
        );
        assert_eq!(
            failure_reason(&EvalRunResult::RisksChanged),
            "The Skill changed while you were confirming. Run again."
        );
        assert_eq!(
            failure_reason(&EvalRunResult::Failed {
                reason: "Could not run claude".into()
            }),
            "Could not run claude"
        );
    }

    #[test]
    fn says_a_run_finished() {
        assert_eq!(
            ended_text("pdf", &EvalRunResult::Completed { summary: summary() }),
            "Evals for pdf finished"
        );
    }

    #[test]
    fn says_why_a_run_stopped_with_its_cost() {
        assert_eq!(
            ended_text(
                "pdf",
                &EvalRunResult::NotComplete {
                    message: "Cancelled".into(),
                    cost_usd: Some(0.0061)
                }
            ),
            "pdf: Cancelled. Cost about US$0.0061, the CLI's figure."
        );
    }

    #[test]
    fn says_why_a_run_failed() {
        assert_eq!(
            ended_text("pdf", &EvalRunResult::NoCli),
            "pdf: The Claude CLI was not found"
        );
    }
}
