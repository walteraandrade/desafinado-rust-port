use super::baseline::RunBaseline;
use super::types::{EvalPrepareResult, PreparedReady};

#[derive(Clone, Debug, PartialEq)]
pub enum EvalDialogState {
    Closed,
    Preparing,
    Confirm {
        prepared: PreparedReady,
        baseline: RunBaseline,
        refreshing: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickEffect {
    None,
    NoEvals,
    NoCli,
    Message { reason: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickSettled {
    pub dialog: EvalDialogState,
    pub effect: PickEffect,
}

fn closed_by(result: &EvalPrepareResult) -> PickSettled {
    PickSettled {
        dialog: EvalDialogState::Closed,
        effect: match result {
            EvalPrepareResult::NoCli => PickEffect::NoCli,
            EvalPrepareResult::Failed { reason } => PickEffect::Message {
                reason: reason.clone(),
            },
            EvalPrepareResult::Ready(_) => PickEffect::None,
        },
    }
}

pub fn settle_ask(result: &EvalPrepareResult) -> PickSettled {
    match result {
        EvalPrepareResult::Ready(prepared) if prepared.case_count == 0 => PickSettled {
            dialog: EvalDialogState::Closed,
            effect: PickEffect::NoEvals,
        },
        EvalPrepareResult::Ready(prepared) => PickSettled {
            dialog: EvalDialogState::Confirm {
                prepared: prepared.clone(),
                baseline: RunBaseline::None,
                refreshing: false,
            },
            effect: PickEffect::None,
        },
        other => closed_by(other),
    }
}

pub fn start_pick(dialog: &EvalDialogState, baseline: RunBaseline) -> EvalDialogState {
    match dialog {
        EvalDialogState::Confirm { prepared, .. } => EvalDialogState::Confirm {
            prepared: prepared.clone(),
            baseline,
            refreshing: true,
        },
        other => other.clone(),
    }
}

pub fn settle_pick(
    dialog: &EvalDialogState,
    baseline: RunBaseline,
    result: &EvalPrepareResult,
) -> PickSettled {
    let stale = match dialog {
        EvalDialogState::Confirm {
            baseline: shown, ..
        } => *shown != baseline,
        _ => true,
    };
    if stale {
        return PickSettled {
            dialog: dialog.clone(),
            effect: PickEffect::None,
        };
    }
    match result {
        EvalPrepareResult::Ready(prepared) => PickSettled {
            dialog: EvalDialogState::Confirm {
                prepared: prepared.clone(),
                baseline,
                refreshing: false,
            },
            effect: PickEffect::None,
        },
        other => closed_by(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared(case_count: usize) -> PreparedReady {
        PreparedReady {
            claude_version: "2.1.319".into(),
            case_count,
            risks: vec![],
            baseline_risks: None,
            versions: vec![],
            same_as_saved: vec![],
        }
    }

    fn ready(case_count: usize) -> EvalPrepareResult {
        EvalPrepareResult::Ready(prepared(case_count))
    }

    const NONE: RunBaseline = RunBaseline::None;
    const FIRST: RunBaseline = RunBaseline::Version { version_id: 1 };
    const SECOND: RunBaseline = RunBaseline::Version { version_id: 2 };

    fn open(baseline: RunBaseline) -> EvalDialogState {
        EvalDialogState::Confirm {
            prepared: prepared(1),
            baseline,
            refreshing: false,
        }
    }

    fn no_effect() -> PickEffect {
        PickEffect::None
    }

    #[test]
    fn marks_the_open_dialog_as_refreshing_for_the_new_baseline() {
        match start_pick(&open(NONE), FIRST) {
            EvalDialogState::Confirm {
                baseline,
                refreshing,
                ..
            } => assert_eq!((baseline, refreshing), (FIRST, true)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn leaves_a_closed_dialog_closed() {
        assert_eq!(
            start_pick(&EvalDialogState::Closed, FIRST),
            EvalDialogState::Closed
        );
    }

    #[test]
    fn shows_the_new_risks_once_the_pick_is_ready() {
        let picked = start_pick(&open(NONE), FIRST);
        let settled = settle_pick(&picked, FIRST, &ready(5));
        assert_eq!(
            settled.dialog,
            EvalDialogState::Confirm {
                prepared: prepared(5),
                baseline: FIRST,
                refreshing: false
            }
        );
        assert_eq!(settled.effect, no_effect());
    }

    #[test]
    fn closes_the_dialog_on_a_failure_for_the_baseline_it_shows() {
        let picked = start_pick(&open(NONE), FIRST);
        assert_eq!(
            settle_pick(
                &picked,
                FIRST,
                &EvalPrepareResult::Failed {
                    reason: "nope".into()
                }
            ),
            PickSettled {
                dialog: EvalDialogState::Closed,
                effect: PickEffect::Message {
                    reason: "nope".into()
                }
            }
        );
        assert_eq!(
            settle_pick(&picked, FIRST, &EvalPrepareResult::NoCli).effect,
            PickEffect::NoCli
        );
    }

    #[test]
    fn ignores_a_stale_failure_a_newer_pick_keeps_the_dialog_open() {
        let newer = start_pick(&start_pick(&open(NONE), FIRST), SECOND);
        let settled = settle_pick(
            &newer,
            FIRST,
            &EvalPrepareResult::Failed {
                reason: "old".into(),
            },
        );
        assert_eq!(settled.dialog, newer);
        assert_eq!(settled.effect, no_effect());
    }

    #[test]
    fn ignores_a_stale_ready_result_too() {
        let newer = start_pick(&start_pick(&open(NONE), FIRST), SECOND);
        assert_eq!(settle_pick(&newer, FIRST, &ready(9)).dialog, newer);
    }

    #[test]
    fn does_not_reopen_a_dialog_that_was_closed_meanwhile() {
        assert_eq!(
            settle_pick(&EvalDialogState::Closed, FIRST, &ready(1)).dialog,
            EvalDialogState::Closed
        );
    }

    #[test]
    fn opens_the_dialog_when_the_skill_has_evals() {
        assert_eq!(
            settle_ask(&ready(2)),
            PickSettled {
                dialog: EvalDialogState::Confirm {
                    prepared: prepared(2),
                    baseline: NONE,
                    refreshing: false
                },
                effect: no_effect()
            }
        );
    }

    #[test]
    fn keeps_the_dialog_closed_and_says_there_are_no_evals() {
        assert_eq!(
            settle_ask(&ready(0)),
            PickSettled {
                dialog: EvalDialogState::Closed,
                effect: PickEffect::NoEvals
            }
        );
    }

    #[test]
    fn closes_on_a_missing_cli_or_a_failure() {
        assert_eq!(
            settle_ask(&EvalPrepareResult::NoCli),
            PickSettled {
                dialog: EvalDialogState::Closed,
                effect: PickEffect::NoCli
            }
        );
        assert_eq!(
            settle_ask(&EvalPrepareResult::Failed {
                reason: "Boom".into()
            }),
            PickSettled {
                dialog: EvalDialogState::Closed,
                effect: PickEffect::Message {
                    reason: "Boom".into()
                }
            }
        );
    }
}
