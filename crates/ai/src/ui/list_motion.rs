use std::collections::BTreeMap;

pub const MOVE_MS: u64 = 260;
pub const ENTER_MS: u64 = 260;
pub const LEAVE_MS: u64 = 180;
pub const EASE_OUT_EXPO: [f64; 4] = [0.16, 1.0, 0.3, 1.0];
pub const EASE_OUT_QUART: [f64; 4] = [0.25, 1.0, 0.5, 1.0];
pub const ENTER_OFFSET_PX: f64 = -6.0;

#[derive(Debug, Clone, PartialEq)]
pub enum ListStep {
    Enter,
    Move { delta: f64 },
}

pub fn plan_list_motion(
    previous: &BTreeMap<String, f64>,
    next: &BTreeMap<String, f64>,
    reduced_motion: bool,
) -> BTreeMap<String, ListStep> {
    if previous.is_empty() || reduced_motion {
        return BTreeMap::new();
    }
    next.iter()
        .filter_map(|(id, after)| match previous.get(id) {
            None => Some((id.clone(), ListStep::Enter)),
            Some(before) => {
                let delta = before - after;
                (delta.abs() >= 1.0).then(|| (id.clone(), ListStep::Move { delta }))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tops(entries: &[(&str, f64)]) -> BTreeMap<String, f64> {
        entries
            .iter()
            .map(|(id, top)| ((*id).to_owned(), *top))
            .collect()
    }

    #[test]
    fn animates_new_items_in_and_moved_items_by_their_offset() {
        let plan = plan_list_motion(
            &tops(&[("a", 0.0), ("b", 40.0), ("c", 80.0)]),
            &tops(&[("a", 0.0), ("c", 40.0), ("d", 80.0)]),
            false,
        );
        assert_eq!(plan.get("a"), None);
        assert_eq!(plan.get("c"), Some(&ListStep::Move { delta: 40.0 }));
        assert_eq!(plan.get("d"), Some(&ListStep::Enter));
    }

    #[test]
    fn ignores_sub_pixel_moves_the_first_measure_and_reduced_motion() {
        assert!(plan_list_motion(&tops(&[("a", 0.0)]), &tops(&[("a", 0.5)]), false).is_empty());
        assert!(plan_list_motion(&BTreeMap::new(), &tops(&[("a", 0.0)]), false).is_empty());
        assert!(plan_list_motion(&tops(&[("a", 0.0)]), &tops(&[("a", 50.0)]), true).is_empty());
    }
}
