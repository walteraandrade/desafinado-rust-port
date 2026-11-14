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
    fn the_first_side_gets_half_rounded_down_to_four_decimals() {
        assert_eq!(first_run_limit(1.0), Some(0.5));
        assert_eq!(first_run_limit(0.0003), Some(0.0001));
        assert_eq!(first_run_limit(0.0001), None);
    }

    #[test]
    fn the_second_side_gets_what_the_first_left() {
        assert_eq!(second_run_limit(1.0, 0.3), Some(0.7));
        assert_eq!(second_run_limit(1.0, 0.99995), None);
        assert_eq!(second_run_limit(1.0, 2.0), None);
    }
}
