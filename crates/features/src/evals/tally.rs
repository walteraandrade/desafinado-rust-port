use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    pub passed: usize,
    pub total: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub passed: bool,
    pub fired: Option<bool>,
    pub errored: bool,
}

pub fn tally_of(passes: impl IntoIterator<Item = bool>) -> Tally {
    passes.into_iter().fold(
        Tally {
            passed: 0,
            total: 0,
        },
        |tally, passed| Tally {
            passed: tally.passed + usize::from(passed),
            total: tally.total + 1,
        },
    )
}

pub fn sum_tallies(tallies: impl IntoIterator<Item = Tally>) -> Tally {
    tallies.into_iter().fold(
        Tally {
            passed: 0,
            total: 0,
        },
        |sum, tally| Tally {
            passed: sum.passed + tally.passed,
            total: sum.total + tally.total,
        },
    )
}

pub fn arm_tally(runs: &[RunRecord]) -> Tally {
    tally_of(runs.iter().filter(|run| !run.errored).map(|run| run.passed))
}
