use super::baseline::{SummaryBaseline, TODAY_LABEL, version_label};
use super::signals::HelpedOverall;
use super::tally::Tally;
use super::types::ReportPart;
use super::view::{ArmLabels, count_label, left_out_text, skill_arms, unpaired_text};

type IntroFn = Box<dyn Fn(usize, &str) -> String>;

pub struct RunCopy {
    pub intro: IntroFn,
    pub stage_note: Option<&'static str>,
    pub limit_note: &'static str,
    pub today_title: Option<&'static str>,
    pub older_title: Option<&'static str>,
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "Eval" } else { "Evals" }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    None,
    Version,
}

pub fn run_copy(kind: RunKind) -> RunCopy {
    match kind {
        RunKind::None => RunCopy {
            intro: Box::new(|count, cli| {
                format!(
                    "{count} {}, each run with and without the Skill, by the real agent through Claude CLI {cli}.",
                    plural(count)
                )
            }),
            stage_note: None,
            limit_note: "The CLI stops the run at the cost limit.",
            today_title: None,
            older_title: None,
        },
        RunKind::Version => RunCopy {
            intro: Box::new(|count, cli| {
                format!(
                    "{count} {}, each run with today's SKILL.md and with the Version, by the real agent through Claude CLI {cli}.",
                    plural(count)
                )
            }),
            stage_note: Some(
                "Two Stages run, one after the other. The older one has the Version's SKILL.md and today's other files, because History keeps only SKILL.md.",
            ),
            limit_note: "The cost limit covers both runs: the first gets half, the second what is left.",
            today_title: Some("Today's Stage"),
            older_title: Some("The Version's Stage"),
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportButton {
    pub part: ReportPart,
    pub label: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Columns {
    pub first: &'static str,
    pub second: &'static str,
}

#[derive(Clone, Debug)]
pub struct SummaryView {
    pub labels: ArmLabels,
    pub columns: Columns,
    pub compared_note: Option<String>,
    pub reports: Vec<ReportButton>,
    pub fired_sides: u8,
    baseline: SummaryBaseline,
}

const NOT_CHECKED: &str = "Not checked. No Eval has a tool_used Skill grader.";

impl SummaryView {
    pub fn fired_text(&self, first: Option<&Tally>, second: Option<&Tally>) -> String {
        match (&self.baseline, first) {
            (_, None) => NOT_CHECKED.to_string(),
            (SummaryBaseline::None, Some(first)) => {
                format!("{} runs with the Skill", count_label(first))
            }
            (SummaryBaseline::Version { .. }, Some(first)) => format!(
                "{} runs with {}{}",
                count_label(first),
                self.labels.first,
                second.map_or(String::new(), |second| format!(
                    ", {} with {}",
                    count_label(second),
                    self.labels.second
                ))
            ),
        }
    }

    pub fn helped_text(&self, helped: Option<&HelpedOverall>) -> String {
        match (&self.baseline, helped) {
            (SummaryBaseline::None, None) => "No run without the Skill to compare.".to_string(),
            (SummaryBaseline::None, Some(helped)) => format!(
                "{} passes with, {} without",
                count_label(&helped.with),
                count_label(&helped.without)
            ),
            (SummaryBaseline::Version { .. }, None) => {
                "No Eval ran in both Stages to compare.".to_string()
            }
            (SummaryBaseline::Version { .. }, Some(helped)) => format!(
                "{} passes with {}, {} with {}",
                count_label(&helped.with),
                self.labels.first,
                count_label(&helped.without),
                self.labels.second
            ),
        }
    }

    pub fn left_out(&self, count: usize) -> String {
        match self.baseline {
            SummaryBaseline::None => left_out_text(count),
            SummaryBaseline::Version { .. } => unpaired_text(count),
        }
    }
}

pub fn summary_view(
    baseline: &SummaryBaseline,
    format_time: impl Fn(i64) -> String,
) -> SummaryView {
    match baseline {
        SummaryBaseline::None => SummaryView {
            labels: skill_arms(),
            columns: Columns {
                first: "With",
                second: "Without",
            },
            compared_note: None,
            reports: vec![ReportButton {
                part: ReportPart::Today,
                label: "Open report",
            }],
            fired_sides: 1,
            baseline: *baseline,
        },
        SummaryBaseline::Version { at, .. } => {
            let labels = ArmLabels {
                first: TODAY_LABEL.to_string(),
                second: version_label(*at, format_time),
            };
            SummaryView {
                compared_note: Some(format!(
                    "Compared {} with {}. The older Stage uses today's other files, because History keeps only SKILL.md.",
                    labels.first, labels.second
                )),
                labels,
                columns: Columns {
                    first: "Today",
                    second: "Version",
                },
                reports: vec![
                    ReportButton {
                        part: ReportPart::Today,
                        label: "Open today's report",
                    },
                    ReportButton {
                        part: ReportPart::Version,
                        label: "Open the Version's report",
                    },
                ],
                fired_sides: 2,
                baseline: *baseline,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(at: i64) -> String {
        format!("t{at}")
    }

    fn tally(passed: usize, total: usize) -> Tally {
        Tally { passed, total }
    }

    const VERSION: SummaryBaseline = SummaryBaseline::Version {
        version_id: 3,
        at: 9,
    };

    #[test]
    fn keeps_the_skill_wording_with_no_baseline() {
        let view = summary_view(&SummaryBaseline::None, time);
        assert_eq!(
            view.columns,
            Columns {
                first: "With",
                second: "Without"
            }
        );
        assert_eq!(view.compared_note, None);
        assert_eq!(
            view.fired_text(Some(&tally(2, 3)), None),
            "2/3 runs with the Skill"
        );
        assert!(view.fired_text(None, None).contains("Not checked"));
        assert_eq!(
            view.helped_text(Some(&HelpedOverall {
                with: tally(2, 3),
                without: tally(1, 3),
                left_out: 0
            })),
            "2/3 passes with, 1/3 without"
        );
        assert_eq!(
            view.helped_text(None),
            "No run without the Skill to compare."
        );
        assert!(view.left_out(1).contains("no run without the Skill"));
    }

    #[test]
    fn offers_one_report_and_no_second_fired_side() {
        let view = summary_view(&SummaryBaseline::None, time);
        assert_eq!(
            view.reports,
            vec![ReportButton {
                part: ReportPart::Today,
                label: "Open report"
            }]
        );
        assert_eq!(view.fired_sides, 1);
    }

    #[test]
    fn names_todays_skill_md_and_the_version() {
        let view = summary_view(&VERSION, time);
        assert_eq!(
            view.columns,
            Columns {
                first: "Today",
                second: "Version"
            }
        );
        assert_eq!(
            view.labels,
            ArmLabels {
                first: "today's SKILL.md".into(),
                second: "the Version from t9".into()
            }
        );
        assert_eq!(
            view.compared_note.as_deref(),
            Some(
                "Compared today's SKILL.md with the Version from t9. The older Stage uses today's other files, because History keeps only SKILL.md."
            )
        );
    }

    #[test]
    fn answers_fired_per_side_and_helped_as_today_against_the_version() {
        let view = summary_view(&VERSION, time);
        assert_eq!(
            view.fired_text(Some(&tally(2, 2)), Some(&tally(1, 2))),
            "2/2 runs with today's SKILL.md, 1/2 with the Version from t9"
        );
        assert_eq!(
            view.fired_text(Some(&tally(2, 2)), None),
            "2/2 runs with today's SKILL.md"
        );
        assert_eq!(
            view.helped_text(Some(&HelpedOverall {
                with: tally(4, 4),
                without: tally(2, 4),
                left_out: 0
            })),
            "4/4 passes with today's SKILL.md, 2/4 with the Version from t9"
        );
        assert_eq!(
            view.helped_text(None),
            "No Eval ran in both Stages to compare."
        );
        assert!(view.left_out(2).contains("only one Stage"));
    }

    #[test]
    fn offers_a_report_per_side_and_fired_on_both_sides() {
        let view = summary_view(&VERSION, time);
        assert_eq!(
            view.reports,
            vec![
                ReportButton {
                    part: ReportPart::Today,
                    label: "Open today's report"
                },
                ReportButton {
                    part: ReportPart::Version,
                    label: "Open the Version's report"
                }
            ]
        );
        assert_eq!(view.fired_sides, 2);
    }

    #[test]
    fn run_copy_has_no_stage_note_or_titles_without_a_version() {
        let copy = run_copy(RunKind::None);
        assert_eq!(copy.stage_note, None);
        assert_eq!(copy.today_title, None);
        assert_eq!(
            (copy.intro)(1, "2.1.319"),
            "1 Eval, each run with and without the Skill, by the real agent through Claude CLI 2.1.319."
        );
    }

    #[test]
    fn run_copy_says_the_older_stage_uses_todays_other_files_and_how_the_limit_is_shared() {
        let copy = run_copy(RunKind::Version);
        assert!(copy.stage_note.unwrap().contains("today's other files"));
        assert!(copy.limit_note.contains("both runs"));
        assert_eq!(copy.older_title, Some("The Version's Stage"));
        assert!((copy.intro)(2, "x").contains("2 Evals"));
    }
}
