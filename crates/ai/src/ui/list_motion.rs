use std::collections::BTreeMap;

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

#[derive(Debug, Clone, PartialEq)]
pub struct ListChange {
    pub steps: BTreeMap<String, ListStep>,
    pub removed: Vec<(usize, String)>,
}

pub fn plan_list_change(
    previous: &[String],
    next: &[String],
    pitch: f64,
    reduced_motion: bool,
) -> Option<ListChange> {
    if previous == next {
        return None;
    }
    let removed: Vec<(usize, String)> = previous
        .iter()
        .enumerate()
        .filter(|(_, id)| !next.contains(id))
        .map(|(index, id)| (index, id.clone()))
        .collect();
    let tops = |ids: &mut dyn Iterator<Item = &String>| -> BTreeMap<String, f64> {
        ids.enumerate()
            .map(|(index, id)| (id.clone(), pitch * index as f64))
            .collect()
    };
    let before = tops(&mut previous.iter().filter(|id| next.contains(id)));
    let after = tops(&mut next.iter());
    let before = if before.is_empty() && !previous.is_empty() {
        tops(&mut previous.iter())
    } else {
        before
    };
    Some(ListChange {
        steps: plan_list_motion(&before, &after, reduced_motion),
        removed: if reduced_motion { Vec::new() } else { removed },
    })
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

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_removed_row_collapses_and_the_rows_below_do_not_also_glide() {
        let change =
            plan_list_change(&ids(&["a", "b", "c"]), &ids(&["b", "c"]), 40.0, false).unwrap();
        assert_eq!(change.removed, vec![(0, "a".to_owned())]);
        assert!(change.steps.is_empty());
    }

    #[test]
    fn a_swap_glides_both_rows_and_a_new_one_enters() {
        let change =
            plan_list_change(&ids(&["a", "b"]), &ids(&["b", "a", "c"]), 40.0, false).unwrap();
        assert_eq!(
            change.steps.get("a"),
            Some(&ListStep::Move { delta: -40.0 })
        );
        assert_eq!(change.steps.get("b"), Some(&ListStep::Move { delta: 40.0 }));
        assert_eq!(change.steps.get("c"), Some(&ListStep::Enter));
        assert!(change.removed.is_empty());
    }

    #[test]
    fn the_same_order_plans_nothing_and_reduced_motion_removes_nothing() {
        assert_eq!(
            plan_list_change(&ids(&["a"]), &ids(&["a"]), 40.0, false),
            None
        );
        let change = plan_list_change(&ids(&["a", "b"]), &ids(&["b"]), 40.0, true).unwrap();
        assert!(change.steps.is_empty() && change.removed.is_empty());
    }

    #[test]
    fn several_removals_and_a_reorder_plan_from_the_rows_that_stay() {
        let change =
            plan_list_change(&ids(&["a", "b", "c", "d"]), &ids(&["d", "c"]), 40.0, false).unwrap();
        assert_eq!(
            change.removed,
            vec![(0, "a".to_owned()), (1, "b".to_owned())]
        );
        assert_eq!(
            change.steps.get("c"),
            Some(&ListStep::Move { delta: -40.0 })
        );
        assert_eq!(change.steps.get("d"), Some(&ListStep::Move { delta: 40.0 }));
    }

    #[test]
    fn replacing_every_row_lets_the_new_ones_enter() {
        let change = plan_list_change(&ids(&["a"]), &ids(&["b"]), 40.0, false).unwrap();
        assert_eq!(change.removed, vec![(0, "a".to_owned())]);
        assert_eq!(change.steps.get("b"), Some(&ListStep::Enter));
    }
}
