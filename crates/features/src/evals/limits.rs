const STEP: f64 = 10_000.0;
const MIN_LIMIT_USD: f64 = 0.0001;

pub const FIRST_SIDE_NOTE: &str = "half for each side";
pub const SECOND_SIDE_NOTE: &str = "what the first side left";
pub const NOTHING_LEFT_NOTE: &str = "nothing left for the second side";
pub const LIMIT_TOO_SMALL: &str = "The cost limit is too small to share between two runs";

fn floor4(value: f64) -> f64 {
    (value * STEP).floor() / STEP
}

pub fn first_run_limit(max_cost_usd: f64) -> Option<f64> {
    let half = floor4(max_cost_usd / 2.0);
    (half >= MIN_LIMIT_USD).then_some(half)
}

pub fn second_run_limit(max_cost_usd: f64, spent_usd: f64) -> Option<f64> {
    let left = floor4(max_cost_usd - spent_usd);
    (left >= MIN_LIMIT_USD).then_some(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_the_first_run_half_and_rounds_down() {
        assert_eq!(first_run_limit(0.3), Some(0.15));
        assert_eq!(first_run_limit(1.0), Some(0.5));
    }

    #[test]
    fn refuses_a_limit_whose_half_is_below_the_smallest_the_cli_is_given() {
        assert_eq!(first_run_limit(0.00015), None);
        assert_eq!(first_run_limit(0.0002), Some(0.0001));
    }

    #[test]
    fn never_gives_the_first_run_more_than_half_of_the_users_limit() {
        for max in [0.0002, 0.00035, 0.3, 0.7777, 5.0] {
            assert!(first_run_limit(max).unwrap_or(0.0) <= max / 2.0);
        }
    }

    #[test]
    fn gives_the_second_run_what_the_first_left_of_the_whole_limit() {
        assert_eq!(second_run_limit(0.3, 0.01), Some(0.29));
        assert_eq!(second_run_limit(1.0, 0.0027), Some(0.9973));
    }

    #[test]
    fn never_lets_the_two_runs_add_up_to_more_than_the_limit() {
        let spent = 0.123456;
        let second = second_run_limit(0.3, spent).unwrap_or(0.0);
        assert!(spent + second <= 0.3);
    }

    #[test]
    fn does_not_start_the_second_run_when_nothing_is_left() {
        assert_eq!(second_run_limit(0.3, 0.3), None);
        assert_eq!(second_run_limit(0.3, 0.31), None);
    }
}
