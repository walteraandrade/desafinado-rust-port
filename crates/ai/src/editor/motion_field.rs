use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeMomentKind {
    Land,
    Settle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MomentDraft {
    Reveal { id: String },
    Land { from: usize, to: usize },
    Settle { from: usize, to: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    pub draft: MomentDraft,
    pub serial: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MotionState {
    pub moments: Vec<Moment>,
    pub next_serial: u64,
}

pub const REVEAL_DURATION_MS: u64 = 1100;
pub const LAND_DURATION_MS: u64 = 1500;
pub const SETTLE_DURATION_MS: u64 = 500;

pub const fn moment_duration_ms(draft: &MomentDraft) -> u64 {
    match draft {
        MomentDraft::Reveal { .. } => REVEAL_DURATION_MS,
        MomentDraft::Land { .. } => LAND_DURATION_MS,
        MomentDraft::Settle { .. } => SETTLE_DURATION_MS,
    }
}

pub fn range_moment(kind: RangeMomentKind, from: usize, to: usize) -> Option<MomentDraft> {
    (from < to).then_some(match kind {
        RangeMomentKind::Land => MomentDraft::Land { from, to },
        RangeMomentKind::Settle => MomentDraft::Settle { from, to },
    })
}

pub fn update_motion(
    state: &MotionState,
    expired: &BTreeSet<u64>,
    doc_changed: bool,
    suggestion_present: impl Fn(&str) -> bool,
    new_drafts: Vec<MomentDraft>,
) -> MotionState {
    let kept: Vec<Moment> = state
        .moments
        .iter()
        .filter(|moment| !expired.contains(&moment.serial))
        .filter(|moment| match &moment.draft {
            MomentDraft::Reveal { id } => suggestion_present(id),
            _ => !doc_changed,
        })
        .cloned()
        .collect();
    let added: Vec<Moment> = new_drafts
        .into_iter()
        .enumerate()
        .map(|(index, draft)| Moment {
            draft,
            serial: state.next_serial + index as u64,
        })
        .collect();
    if added.is_empty() && kept.len() == state.moments.len() {
        return state.clone();
    }
    MotionState {
        next_serial: state.next_serial + added.len() as u64,
        moments: kept.into_iter().chain(added).collect(),
    }
}

pub fn revealing_ids(state: &MotionState) -> BTreeSet<String> {
    state
        .moments
        .iter()
        .filter_map(|moment| match &moment.draft {
            MomentDraft::Reveal { id } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct ExpiryScheduler {
    deadlines: BTreeMap<u64, u64>,
}

impl ExpiryScheduler {
    pub fn schedule(&mut self, moments: &[Moment], now_ms: u64) {
        for moment in moments {
            self.deadlines
                .entry(moment.serial)
                .or_insert(now_ms + moment_duration_ms(&moment.draft));
        }
    }

    pub fn due(&mut self, now_ms: u64) -> Vec<u64> {
        let mut due: Vec<(u64, u64)> = self
            .deadlines
            .iter()
            .filter(|(_, deadline)| **deadline <= now_ms)
            .map(|(serial, deadline)| (*deadline, *serial))
            .collect();
        due.sort_unstable();
        for (_, serial) in &due {
            self.deadlines.remove(serial);
        }
        due.into_iter().map(|(_, serial)| serial).collect()
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.deadlines.values().min().copied()
    }

    pub fn destroy(&mut self) {
        self.deadlines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reveal(id: &str) -> MomentDraft {
        MomentDraft::Reveal { id: id.into() }
    }

    fn step(state: &MotionState, doc_changed: bool, drafts: Vec<MomentDraft>) -> MotionState {
        update_motion(state, &BTreeSet::new(), doc_changed, |_| true, drafts)
    }

    fn drafts(state: &MotionState) -> Vec<MomentDraft> {
        state.moments.iter().map(|m| m.draft.clone()).collect()
    }

    #[test]
    fn records_a_reveal_when_a_replacement_arrives() {
        let state = step(&MotionState::default(), false, vec![reveal("a")]);
        assert_eq!(drafts(&state), [reveal("a")]);
    }

    #[test]
    fn records_where_accepted_text_landed() {
        let land = range_moment(RangeMomentKind::Land, 0, "Opening".len()).unwrap();
        let state = step(&MotionState::default(), true, vec![land.clone()]);
        assert_eq!(drafts(&state), [land]);
    }

    #[test]
    fn records_every_landing_of_accept_all() {
        let state = step(
            &MotionState::default(),
            true,
            vec![
                MomentDraft::Land { from: 0, to: 7 },
                MomentDraft::Land { from: 9, to: 12 },
            ],
        );
        assert_eq!(state.moments.len(), 2);
        assert_eq!(state.moments[1].serial, 1);
    }

    #[test]
    fn records_no_landing_when_the_accepted_replacement_is_empty() {
        assert_eq!(range_moment(RangeMomentKind::Land, 3, 3), None);
    }

    #[test]
    fn records_a_settle_where_a_rejected_suggestion_was() {
        assert_eq!(
            range_moment(RangeMomentKind::Settle, 23, 29),
            Some(MomentDraft::Settle { from: 23, to: 29 })
        );
    }

    #[test]
    fn drops_a_moment_when_it_expires() {
        let state = step(
            &MotionState::default(),
            true,
            vec![MomentDraft::Land { from: 0, to: 4 }],
        );
        let serial = state.moments[0].serial;
        let next = update_motion(&state, &BTreeSet::from([serial]), false, |_| true, vec![]);
        assert_eq!(next.moments, vec![]);
    }

    #[test]
    fn ends_landings_as_soon_as_the_writer_edits_again() {
        let landed = step(
            &MotionState::default(),
            true,
            vec![MomentDraft::Land { from: 0, to: 4 }],
        );
        assert_eq!(step(&landed, true, vec![]).moments, vec![]);
    }

    #[test]
    fn keeps_landings_through_selection_moves() {
        let landed = step(
            &MotionState::default(),
            true,
            vec![MomentDraft::Land { from: 0, to: 4 }],
        );
        assert_eq!(step(&landed, false, vec![]), landed);
    }

    #[test]
    fn keeps_a_reveal_only_while_its_suggestion_exists() {
        let state = step(&MotionState::default(), false, vec![reveal("a")]);
        let gone = update_motion(&state, &BTreeSet::new(), false, |_| false, vec![]);
        assert_eq!(gone.moments, vec![]);
        assert_eq!(gone.next_serial, 1);
        assert_eq!(revealing_ids(&state), BTreeSet::from(["a".to_owned()]));
    }

    #[test]
    fn reads_as_empty_by_default() {
        assert_eq!(MotionState::default().moments, vec![]);
    }

    fn moment(serial: u64, draft: MomentDraft) -> Moment {
        Moment { draft, serial }
    }

    #[test]
    fn expires_each_moment_once_after_its_own_duration() {
        let reveal_moment = moment(0, reveal("a"));
        let settle = moment(1, MomentDraft::Settle { from: 0, to: 5 });
        let mut scheduler = ExpiryScheduler::default();
        scheduler.schedule(&[reveal_moment.clone(), settle.clone()], 0);
        scheduler.schedule(&[reveal_moment, settle], 0);
        assert_eq!(scheduler.due(499), Vec::<u64>::new());
        assert_eq!(scheduler.due(500), [1]);
        assert_eq!(scheduler.due(1100), [0]);
        assert_eq!(scheduler.due(5000), Vec::<u64>::new());
    }

    #[test]
    fn expires_nothing_after_it_is_destroyed() {
        let mut scheduler = ExpiryScheduler::default();
        scheduler.schedule(&[moment(0, reveal("a"))], 0);
        scheduler.destroy();
        assert_eq!(scheduler.due(10_000), Vec::<u64>::new());
        assert_eq!(scheduler.next_deadline(), None);
    }

    #[test]
    fn names_the_duration_of_each_kind() {
        assert_eq!(moment_duration_ms(&reveal("a")), 1100);
        assert_eq!(
            moment_duration_ms(&MomentDraft::Land { from: 0, to: 1 }),
            1500
        );
        assert_eq!(
            moment_duration_ms(&MomentDraft::Settle { from: 0, to: 1 }),
            500
        );
    }
}
