use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalSettings {
    pub runs: f64,
    pub model: String,
    pub max_cost_usd: f64,
}

pub const MIN_RUNS: u32 = 1;
pub const MAX_RUNS: u32 = 5;
pub const MAX_COST_LIMIT_USD: f64 = 5.0;

pub fn default_eval_settings() -> EvalSettings {
    EvalSettings {
        runs: 3.0,
        model: "haiku".to_string(),
        max_cost_usd: 1.0,
    }
}

pub fn is_valid_model(model: &str) -> bool {
    let mut chars = model.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

pub fn check_eval_settings(settings: &EvalSettings) -> Vec<String> {
    let EvalSettings {
        runs,
        model,
        max_cost_usd,
    } = settings;
    let mut problems = Vec::new();
    if !(runs.fract() == 0.0 && *runs >= f64::from(MIN_RUNS) && *runs <= f64::from(MAX_RUNS)) {
        problems.push(format!(
            "Runs must be a whole number from {MIN_RUNS} to {MAX_RUNS}"
        ));
    }
    if !is_valid_model(model) {
        problems
            .push("Model starts with a letter, then letters, digits, \".\" and \"-\"".to_string());
    }
    if !(max_cost_usd.is_finite() && *max_cost_usd > 0.0 && *max_cost_usd <= MAX_COST_LIMIT_USD) {
        problems.push(format!(
            "Cost limit must be above 0 and at most US${MAX_COST_LIMIT_USD}"
        ));
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(change: impl FnOnce(&mut EvalSettings)) -> EvalSettings {
        let mut settings = default_eval_settings();
        change(&mut settings);
        settings
    }

    #[test]
    fn accepts_the_defaults() {
        assert!(check_eval_settings(&default_eval_settings()).is_empty());
    }

    #[test]
    fn refuses_bad_runs() {
        for runs in [0.0, 6.0, 1.5, f64::NAN] {
            assert!(
                !check_eval_settings(&with(|s| s.runs = runs)).is_empty(),
                "{runs}"
            );
        }
    }

    #[test]
    fn refuses_bad_models() {
        for model in ["--allow-tools", "", "1haiku", "a b", "a=b", "-x", "a/b"] {
            assert!(
                !check_eval_settings(&with(|s| s.model = model.into())).is_empty(),
                "{model}"
            );
        }
    }

    #[test]
    fn accepts_good_models() {
        for model in ["haiku", "sonnet", "claude-haiku-4.5"] {
            assert!(check_eval_settings(&with(|s| s.model = model.into())).is_empty());
        }
    }

    #[test]
    fn refuses_bad_cost_limits() {
        for limit in [0.0, -1.0, 5.01, f64::NAN, f64::INFINITY] {
            assert!(
                !check_eval_settings(&with(|s| s.max_cost_usd = limit)).is_empty(),
                "{limit}"
            );
        }
    }

    #[test]
    fn accepts_a_cost_limit_of_5() {
        assert!(check_eval_settings(&with(|s| s.max_cost_usd = 5.0)).is_empty());
    }
}
