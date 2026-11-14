use super::baseline::{SummaryBaseline, TODAY_LABEL, version_label};
use super::signals::SuspiciousKind;
use super::tally::Tally;
use crate::text::to_fixed;

pub fn count_label(tally: &Tally) -> String {
    format!("{}/{}", tally.passed, tally.total)
}

const SMALL_COST_USD: f64 = 0.1;

fn amount(cost_usd: f64) -> String {
    if cost_usd == 0.0 {
        "0".to_string()
    } else if cost_usd < SMALL_COST_USD {
        to_fixed(cost_usd, 4)
    } else {
        to_fixed(cost_usd, 2)
    }
}

pub fn cost_estimate(cost_usd: f64) -> String {
    format!("about US${}", amount(cost_usd))
}

pub fn stopped_text(message: &str, cost_usd: Option<f64>) -> String {
    match cost_usd {
        None => message.to_string(),
        Some(cost) => format!("{message}. Cost {}, the CLI's figure.", cost_estimate(cost)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmLabels {
    pub first: String,
    pub second: String,
}

pub fn skill_arms() -> ArmLabels {
    ArmLabels {
        first: "with the Skill".to_string(),
        second: "without the Skill".to_string(),
    }
}

pub fn arm_labels_of(baseline: &SummaryBaseline, format_time: impl Fn(i64) -> String) -> ArmLabels {
    match baseline {
        SummaryBaseline::None => skill_arms(),
        SummaryBaseline::Version { at, .. } => ArmLabels {
            first: TODAY_LABEL.to_string(),
            second: version_label(*at, format_time),
        },
    }
}

pub fn suspicious_text(kind: SuspiciousKind, labels: &ArmLabels) -> String {
    let ArmLabels { first, second } = labels;
    match kind {
        SuspiciousKind::PassesInBoth => {
            format!("passes {first} and {second}, so it does not tell them apart")
        }
        SuspiciousKind::FailsInBoth => {
            format!("fails {first} and {second}, so it is likely broken")
        }
        SuspiciousKind::RunsDisagree => "its runs disagree, so it is noisy".to_string(),
    }
}

fn count_words_of(count: usize, one: &str, many: &str) -> String {
    if count == 1 { one } else { many }.to_string()
}

pub fn errored_text(count: usize) -> String {
    format!(
        "{count} {} ended on an error and {} not counted",
        count_words_of(count, "run", "runs"),
        count_words_of(count, "is", "are")
    )
}

pub fn left_out_text(count: usize) -> String {
    format!(
        "{count} {} no run without the Skill and {} left out of the comparison",
        count_words_of(count, "Eval has", "Evals have"),
        count_words_of(count, "is", "are")
    )
}

pub fn unpaired_text(count: usize) -> String {
    format!(
        "{count} {} a run in only one Stage and {} left out of the comparison",
        count_words_of(count, "Eval has", "Evals have"),
        count_words_of(count, "is", "are")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_label_shows_passes_over_entries() {
        assert_eq!(
            count_label(&Tally {
                passed: 2,
                total: 3
            }),
            "2/3"
        );
        assert_eq!(
            count_label(&Tally {
                passed: 0,
                total: 0
            }),
            "0/0"
        );
    }

    #[test]
    fn cost_estimate_marks_the_cli_figure_as_an_estimate() {
        assert_eq!(cost_estimate(0.00265497), "about US$0.0027");
        assert_eq!(cost_estimate(0.5), "about US$0.50");
        assert_eq!(cost_estimate(0.0), "about US$0");
    }

    #[test]
    fn stopped_text_adds_the_cost_when_it_is_known() {
        assert_eq!(
            stopped_text("Cancelled", Some(0.0015)),
            "Cancelled. Cost about US$0.0015, the CLI's figure."
        );
        assert_eq!(stopped_text("Cancelled", None), "Cancelled");
    }

    #[test]
    fn suspicious_text_names_each_kind_in_plain_words() {
        assert_eq!(
            suspicious_text(SuspiciousKind::PassesInBoth, &skill_arms()),
            "passes with the Skill and without the Skill, so it does not tell them apart"
        );
        assert!(
            suspicious_text(SuspiciousKind::FailsInBoth, &skill_arms()).contains("likely broken")
        );
        assert!(suspicious_text(SuspiciousKind::RunsDisagree, &skill_arms()).contains("noisy"));
    }

    #[test]
    fn suspicious_text_takes_the_arm_labels_for_another_pair_of_arms() {
        assert!(
            suspicious_text(
                SuspiciousKind::PassesInBoth,
                &ArmLabels {
                    first: "in this Version".into(),
                    second: "in the older Version".into()
                }
            )
            .contains("passes in this Version and in the older Version")
        );
    }

    #[test]
    fn errored_and_left_out_text_count_in_words() {
        assert_eq!(
            errored_text(1),
            "1 run ended on an error and is not counted"
        );
        assert!(errored_text(2).contains("2 runs"));
        assert!(left_out_text(1).contains("1 Eval has no run without the Skill"));
        assert!(left_out_text(3).contains("3 Evals have"));
    }

    #[test]
    fn arm_labels_keep_the_skill_labels_with_no_baseline() {
        assert_eq!(
            arm_labels_of(&SummaryBaseline::None, |at| at.to_string()),
            ArmLabels {
                first: "with the Skill".into(),
                second: "without the Skill".into()
            }
        );
    }

    #[test]
    fn arm_labels_name_todays_skill_md_and_the_version() {
        let baseline = SummaryBaseline::Version {
            version_id: 3,
            at: 9,
        };
        assert_eq!(
            arm_labels_of(&baseline, |at| format!("t{at}")),
            ArmLabels {
                first: "today's SKILL.md".into(),
                second: "the Version from t9".into()
            }
        );
    }

    #[test]
    fn arm_labels_fit_the_suspicious_sentences() {
        let baseline = SummaryBaseline::Version {
            version_id: 3,
            at: 9,
        };
        let labels = arm_labels_of(&baseline, |at| format!("t{at}"));
        assert_eq!(
            suspicious_text(SuspiciousKind::PassesInBoth, &labels),
            "passes today's SKILL.md and the Version from t9, so it does not tell them apart"
        );
    }

    #[test]
    fn unpaired_text_counts_the_evals_left_out() {
        assert_eq!(
            unpaired_text(1),
            "1 Eval has a run in only one Stage and is left out of the comparison"
        );
        assert!(unpaired_text(2).contains("2 Evals have"));
    }
}
