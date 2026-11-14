use super::report::CaseTally;
use super::tally::{RunRecord, Tally, arm_tally, sum_tallies};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuspiciousKind {
    PassesInBoth,
    FailsInBoth,
    RunsDisagree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseSignals {
    pub name: String,
    pub fired: Option<Tally>,
    pub fired_second: Option<Tally>,
    pub errored: usize,
    pub suspicious: Vec<SuspiciousKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelpedOverall {
    pub with: Tally,
    pub without: Tally,
    pub left_out: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiredSide {
    First,
    Second,
}

fn fired_of(runs: Option<&[RunRecord]>) -> Option<Tally> {
    let checked: Vec<&RunRecord> = runs?
        .iter()
        .filter(|run| run.fired.is_some() && !run.errored)
        .collect();
    (!checked.is_empty()).then(|| Tally {
        passed: checked.iter().filter(|run| run.fired == Some(true)).count(),
        total: checked.len(),
    })
}

fn all_pass(tally: &Tally) -> bool {
    tally.total > 0 && tally.passed == tally.total
}

fn none_pass(tally: &Tally) -> bool {
    tally.total > 0 && tally.passed == 0
}

fn mixed(tally: &Tally) -> bool {
    tally.passed > 0 && tally.passed < tally.total
}

fn suspicious_of(first: Tally, second: Option<Tally>) -> Vec<SuspiciousKind> {
    let arms: Vec<Tally> = std::iter::once(first).chain(second).collect();
    let pair = second.is_some();
    let mut kinds = Vec::new();
    if pair && arms.iter().all(all_pass) {
        kinds.push(SuspiciousKind::PassesInBoth);
    }
    if pair && arms.iter().all(none_pass) {
        kinds.push(SuspiciousKind::FailsInBoth);
    }
    if arms.iter().any(mixed) {
        kinds.push(SuspiciousKind::RunsDisagree);
    }
    kinds
}

fn errored_in(runs: Option<&[RunRecord]>) -> usize {
    runs.map_or(0, |runs| runs.iter().filter(|run| run.errored).count())
}

pub fn signals_of(name: &str, first: &[RunRecord], second: Option<&[RunRecord]>) -> CaseSignals {
    CaseSignals {
        name: name.to_string(),
        fired: fired_of(Some(first)),
        fired_second: fired_of(second),
        errored: errored_in(Some(first)) + errored_in(second),
        suspicious: suspicious_of(arm_tally(first), second.map(arm_tally)),
    }
}

pub fn case_signals(case: &CaseTally) -> CaseSignals {
    signals_of(&case.name, &case.runs.with, case.runs.without.as_deref())
}

pub fn fired_overall(cases: &[CaseTally], side: FiredSide) -> Option<Tally> {
    let tallies: Vec<Tally> = cases
        .iter()
        .filter_map(|case| {
            let signals = case_signals(case);
            match side {
                FiredSide::First => signals.fired,
                FiredSide::Second => signals.fired_second,
            }
        })
        .collect();
    (!tallies.is_empty()).then(|| sum_tallies(tallies))
}

pub fn helped_overall(cases: &[CaseTally]) -> Option<HelpedOverall> {
    let paired: Vec<(Tally, Tally)> = cases
        .iter()
        .filter_map(|case| match case.without {
            Some(without) if case.with.total != 0 => Some((case.with, without)),
            _ => None,
        })
        .collect();
    (!paired.is_empty()).then(|| HelpedOverall {
        with: sum_tallies(paired.iter().map(|(with, _)| *with)),
        without: sum_tallies(paired.iter().map(|(_, without)| *without)),
        left_out: cases.len() - paired.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evals::report::CaseRuns;

    fn run(passed: bool, fired: Option<bool>, errored: bool) -> RunRecord {
        RunRecord {
            passed,
            fired,
            errored,
        }
    }

    fn pass(passed: bool) -> RunRecord {
        run(passed, None, false)
    }

    fn fire(passed: bool, fired: bool) -> RunRecord {
        run(passed, Some(fired), false)
    }

    fn item(with: Vec<RunRecord>, without: Option<Vec<RunRecord>>) -> CaseTally {
        named("case", with, without)
    }

    fn named(name: &str, with: Vec<RunRecord>, without: Option<Vec<RunRecord>>) -> CaseTally {
        CaseTally {
            name: name.into(),
            with: arm_tally(&with),
            without: without.as_deref().map(arm_tally),
            runs: CaseRuns { with, without },
        }
    }

    fn tally(passed: usize, total: usize) -> Tally {
        Tally { passed, total }
    }

    #[test]
    fn counts_the_runs_in_the_arm_with_the_skill_where_it_fired() {
        let signals = case_signals(&item(
            vec![fire(true, true), fire(true, false), fire(false, true)],
            Some(vec![pass(false)]),
        ));
        assert_eq!(signals.fired, Some(tally(2, 3)));
    }

    #[test]
    fn fired_is_not_checked_when_the_case_has_no_tool_used_skill_grader() {
        assert_eq!(
            case_signals(&item(vec![pass(true)], Some(vec![pass(false)]))).fired,
            None
        );
    }

    #[test]
    fn fired_is_zero_not_unchecked_when_the_skill_never_fired() {
        assert_eq!(
            case_signals(&item(vec![fire(true, false)], Some(vec![pass(false)]))).fired,
            Some(tally(0, 1))
        );
    }

    #[test]
    fn leaves_out_runs_that_ended_on_an_error() {
        assert_eq!(
            case_signals(&item(
                vec![run(false, Some(false), true), fire(true, true)],
                None
            ))
            .fired,
            Some(tally(1, 1))
        );
    }

    #[test]
    fn counts_errored_runs_across_both_arms_and_keeps_them_out_of_suspicious() {
        let signals = case_signals(&item(
            vec![pass(true), run(false, None, true)],
            Some(vec![pass(true), pass(true)]),
        ));
        assert_eq!(signals.errored, 1);
        assert_eq!(signals.suspicious, vec![SuspiciousKind::PassesInBoth]);
        assert_eq!(
            case_signals(&item(
                vec![run(false, None, true)],
                Some(vec![run(false, None, true)])
            ))
            .suspicious,
            vec![]
        );
    }

    #[test]
    fn flags_passes_in_both_arms() {
        assert_eq!(
            case_signals(&item(
                vec![pass(true), pass(true)],
                Some(vec![pass(true), pass(true)])
            ))
            .suspicious,
            vec![SuspiciousKind::PassesInBoth]
        );
    }

    #[test]
    fn flags_fails_in_both_arms() {
        assert_eq!(
            case_signals(&item(vec![pass(false)], Some(vec![pass(false)]))).suspicious,
            vec![SuspiciousKind::FailsInBoth]
        );
    }

    #[test]
    fn flags_runs_that_disagree_within_an_arm() {
        assert_eq!(
            case_signals(&item(
                vec![pass(true), pass(false)],
                Some(vec![pass(false), pass(false)])
            ))
            .suspicious,
            vec![SuspiciousKind::RunsDisagree]
        );
    }

    #[test]
    fn flags_noise_in_either_arm_even_when_the_other_is_clean() {
        assert_eq!(
            case_signals(&item(
                vec![pass(true), pass(false)],
                Some(vec![pass(true), pass(true)])
            ))
            .suspicious,
            vec![SuspiciousKind::RunsDisagree]
        );
        assert_eq!(
            case_signals(&item(
                vec![pass(true), pass(true)],
                Some(vec![pass(true), pass(false)])
            ))
            .suspicious,
            vec![SuspiciousKind::RunsDisagree]
        );
    }

    #[test]
    fn keeps_the_kinds_apart() {
        let cases = [
            item(vec![pass(true)], Some(vec![pass(true)])),
            item(vec![pass(false)], Some(vec![pass(false)])),
            item(
                vec![pass(true), pass(false)],
                Some(vec![pass(true), pass(false)]),
            ),
        ];
        let counts: Vec<_> = cases
            .iter()
            .map(|c| case_signals(c).suspicious.len())
            .collect();
        assert_eq!(counts, vec![1, 1, 1]);
    }

    #[test]
    fn does_not_flag_a_case_that_tells_the_arms_apart() {
        assert_eq!(
            case_signals(&item(
                vec![pass(true), pass(true)],
                Some(vec![pass(false), pass(false)])
            ))
            .suspicious,
            vec![]
        );
    }

    #[test]
    fn judges_one_arm_only_on_disagreement() {
        assert_eq!(
            case_signals(&item(vec![pass(true), pass(true)], None)).suspicious,
            vec![]
        );
        assert_eq!(
            case_signals(&item(vec![pass(true), pass(false)], None)).suspicious,
            vec![SuspiciousKind::RunsDisagree]
        );
    }

    #[test]
    fn flags_nothing_for_an_arm_with_no_runs() {
        assert_eq!(case_signals(&item(vec![], Some(vec![]))).suspicious, vec![]);
    }

    #[test]
    fn works_on_any_pair_of_arms() {
        let signals = signals_of(
            "older",
            &[pass(true), pass(true)],
            Some(&[pass(true), pass(true)]),
        );
        assert_eq!(signals.name, "older");
        assert_eq!(signals.suspicious, vec![SuspiciousKind::PassesInBoth]);
    }

    #[test]
    fn fired_overall_sums_the_checked_cases_and_ignores_the_unchecked() {
        assert_eq!(
            fired_overall(
                &[
                    item(
                        vec![fire(true, true), fire(true, false)],
                        Some(vec![pass(false)])
                    ),
                    item(vec![pass(true)], Some(vec![pass(false)])),
                    item(vec![fire(true, true)], Some(vec![pass(false)])),
                ],
                FiredSide::First
            ),
            Some(tally(2, 3))
        );
    }

    #[test]
    fn fired_overall_is_none_when_no_case_checks_it() {
        assert_eq!(
            fired_overall(
                &[item(vec![pass(true)], Some(vec![pass(false)]))],
                FiredSide::First
            ),
            None
        );
    }

    #[test]
    fn helped_overall_compares_only_the_cases_that_have_both_arms_and_counts_the_rest() {
        assert_eq!(
            helped_overall(&[
                item(
                    vec![pass(true), pass(true)],
                    Some(vec![pass(false), pass(true)])
                ),
                item(vec![pass(true)], None),
            ]),
            Some(HelpedOverall {
                with: tally(2, 2),
                without: tally(1, 2),
                left_out: 1
            })
        );
    }

    #[test]
    fn helped_overall_is_none_when_no_case_has_a_second_arm() {
        assert_eq!(helped_overall(&[item(vec![pass(true)], None)]), None);
    }

    #[test]
    fn counts_the_second_side_only_when_its_runs_carry_the_indicator() {
        let signals = case_signals(&item(
            vec![fire(true, true)],
            Some(vec![fire(true, true), fire(true, false)]),
        ));
        assert_eq!(signals.fired_second, Some(tally(1, 2)));
        assert_eq!(
            case_signals(&item(vec![fire(true, true)], Some(vec![pass(true)]))).fired_second,
            None
        );
        assert_eq!(
            case_signals(&item(vec![fire(true, true)], None)).fired_second,
            None
        );
    }

    #[test]
    fn sums_either_side_over_the_cases() {
        let cases = [
            named("a", vec![fire(true, true)], Some(vec![fire(true, false)])),
            named("b", vec![fire(true, true)], Some(vec![fire(true, true)])),
        ];
        assert_eq!(fired_overall(&cases, FiredSide::First), Some(tally(2, 2)));
        assert_eq!(fired_overall(&cases, FiredSide::Second), Some(tally(1, 2)));
    }

    #[test]
    fn leaves_out_an_eval_that_has_no_run_for_today() {
        let empty = named("only-older", vec![], Some(vec![pass(true)]));
        let paired = named("both", vec![pass(true)], Some(vec![pass(false)]));
        assert_eq!(
            helped_overall(&[paired, empty]),
            Some(HelpedOverall {
                with: tally(1, 1),
                without: tally(0, 1),
                left_out: 1
            })
        );
    }
}
