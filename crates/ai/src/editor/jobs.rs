//! The requests tray: one row for each request the model is working on, and for a request that
//! has just answered. The editor stays free while they run.

use std::time::Duration;

use desafinado_core::suggestions::{
    RefineState, Suggestion, SuggestionsState, TargetSource, change_ranges, instruction_label,
};

/// How long an answered request stays in the tray.
pub const READY_ROW_TIME: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// A request from the ask bubble; Cancel drops it.
    Ask,
    /// A refine of an open Suggestion; Cancel goes back to the shown version.
    Refine,
    /// A Review of the whole Document; Stop ends it.
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobPhase {
    Working,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub phase: JobPhase,
    pub label: String,
    pub detail: String,
}

/// Minutes and seconds, as a stopwatch shows them.
pub fn elapsed_label(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// What part of the Document a request covers.
pub fn scope_label(doc: &str, from: usize, to: usize, source: TargetSource) -> &'static str {
    let before = doc.get(..from).unwrap_or("");
    let after = doc.get(to..).unwrap_or("");
    if before.trim().is_empty() && after.trim().is_empty() {
        return "Whole text";
    }
    source_label(source)
}

fn source_label(source: TargetSource) -> &'static str {
    match source {
        TargetSource::Selection => "Selection",
        TargetSource::Block => "Paragraph",
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// What the tray knows of a request from when it left: its time so far and the scope it covered
/// then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Started {
    pub elapsed: Duration,
    pub scope: &'static str,
}

/// The rows for the requests in flight, oldest first. `started` tells each request's time and
/// scope; a request it does not know shows its source and no time.
pub fn working_jobs(
    state: &SuggestionsState,
    started: impl Fn(&str) -> Option<Started>,
) -> Vec<Job> {
    let mut items: Vec<&Suggestion> = state.items.iter().collect();
    items.sort_by_key(|item| item.sequence);
    items
        .into_iter()
        .filter_map(|item| {
            let (kind, label, scope) = match item {
                Suggestion::Pending(pending) => (
                    JobKind::Ask,
                    instruction_label(&pending.instruction).to_string(),
                    started(&item.id).map_or(source_label(item.source), |known| known.scope),
                ),
                Suggestion::Ready(ready) => match &ready.refine {
                    RefineState::Working { instruction } => (
                        JobKind::Refine,
                        instruction_label(instruction).to_string(),
                        "Refining",
                    ),
                    _ => return None,
                },
                Suggestion::Failed(_) => return None,
            };
            let time = started(&item.id).map(|known| elapsed_label(known.elapsed));
            let detail = [Some(scope.to_string()), time]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            Some(Job {
                id: item.id.clone(),
                kind,
                phase: JobPhase::Working,
                label,
                detail,
            })
        })
        .collect()
}

/// The row for a Review while it runs.
pub fn review_job(added: usize, elapsed: Duration) -> Job {
    let mut detail = format!("Whole text · {}", elapsed_label(elapsed));
    if added > 0 {
        detail.push_str(" · ");
        detail.push_str(&plural(added, "suggestion", "suggestions"));
    }
    Job {
        id: "review".into(),
        kind: JobKind::Review,
        phase: JobPhase::Working,
        label: "Review".into(),
        detail,
    }
}

/// The row for a request that has just answered with Suggestion `id`, or `None` when the answer
/// is gone (accepted, rejected, discarded) or is being refined, which has its own row. `left_out` says some of its changes were dropped
/// because the writer edited those lines meanwhile.
pub fn ready_job(state: &SuggestionsState, id: &str, left_out: bool) -> Option<Job> {
    let ready = state
        .items
        .iter()
        .find(|item| item.id == id)?
        .as_ready()
        .filter(|ready| !matches!(ready.refine, RefineState::Working { .. }))?;
    let changes = change_ranges(ready.from, &ready.diff).len().max(1);
    let mut detail = format!("Ready · {}", plural(changes, "change", "changes"));
    if left_out {
        detail.push_str(", some left out");
    }
    Some(Job {
        id: id.to_string(),
        kind: JobKind::Ask,
        phase: JobPhase::Ready,
        label: instruction_label(&ready.instruction).to_string(),
        detail,
    })
}

/// Where the tray lays out the mark of each working request, by request, so
/// the marks can breathe in a view of their own over the tray. `B` is the
/// place a mark takes, such as its bounds in the window.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkSpots<B> {
    spots: Vec<(String, B)>,
}

impl<B> Default for MarkSpots<B> {
    fn default() -> Self {
        Self { spots: Vec::new() }
    }
}

impl<B: PartialEq> MarkSpots<B> {
    /// Notes that the mark of request `id` is laid out `at`. Whether that is
    /// news: a new mark, or one that moved. The same place again is not, so a
    /// still tray asks for nothing.
    pub fn place(&mut self, id: &str, at: B) -> bool {
        match self.spots.iter_mut().find(|(known, _)| known == id) {
            Some((_, known)) if *known == at => false,
            Some((_, known)) => {
                *known = at;
                true
            }
            None => {
                self.spots.push((id.to_string(), at));
                true
            }
        }
    }

    /// Lets go of the marks of requests that `jobs` no longer shows working.
    /// Whether any went.
    pub fn keep_working(&mut self, jobs: &[Job]) -> bool {
        let before = self.spots.len();
        self.spots.retain(|(id, _)| {
            jobs.iter()
                .any(|job| &job.id == id && job.phase == JobPhase::Working)
        });
        self.spots.len() != before
    }

    /// Each mark's request and place, in the order they were first laid out.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &B)> {
        self.spots.iter().map(|(id, at)| (id.as_str(), at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_core::suggestions::{
        Instruction, SuggestionDraft, SuggestionResult, begin_refine, begin_suggestion,
        empty_suggestions, resolve_suggestion,
    };

    const DOC: &str = "One two three.\n\nFour five six.\n";

    fn free(text: &str) -> Instruction {
        Instruction::FreeText { text: text.into() }
    }

    fn ask(id: &str, from: usize, to: usize, source: TargetSource) -> SuggestionDraft {
        SuggestionDraft {
            id: id.into(),
            from,
            to,
            source,
            original: DOC[from..to].into(),
            instruction: free("overall direction"),
            origin: None,
        }
    }

    fn answer(replacement: &str) -> SuggestionResult {
        SuggestionResult {
            replacement: replacement.into(),
            rationale: "Because.".into(),
            unchanged: false,
        }
    }

    #[test]
    fn shows_elapsed_time_as_minutes_and_seconds() {
        assert_eq!(elapsed_label(Duration::from_millis(7_900)), "0:07");
        assert_eq!(elapsed_label(Duration::from_secs(65)), "1:05");
        assert_eq!(elapsed_label(Duration::from_mins(12)), "12:00");
    }

    #[test]
    fn names_a_request_over_all_the_text_whole_text_and_others_by_their_source() {
        assert_eq!(
            scope_label(DOC, 0, DOC.len() - 1, TargetSource::Selection),
            "Whole text"
        );
        assert_eq!(
            scope_label(DOC, 0, 14, TargetSource::Selection),
            "Selection"
        );
        assert_eq!(scope_label(DOC, 16, 30, TargetSource::Block), "Paragraph");
    }

    #[test]
    fn lists_each_request_in_flight_oldest_first_with_its_scope_and_time() {
        let state = begin_suggestion(&empty_suggestions(), ask("b", 16, 30, TargetSource::Block));
        let state = begin_suggestion(&state, ask("a", 0, 14, TargetSource::Selection));
        let jobs = working_jobs(&state, |id| {
            (id == "b").then_some(Started {
                elapsed: Duration::from_secs(12),
                scope: "Paragraph",
            })
        });
        let rows: Vec<(&str, &str)> = jobs
            .iter()
            .map(|job| (job.id.as_str(), job.detail.as_str()))
            .collect();
        assert_eq!(rows, [("b", "Paragraph · 0:12"), ("a", "Selection")]);
        assert!(jobs.iter().all(|job| job.kind == JobKind::Ask));
        assert_eq!(jobs[0].label, "overall direction");
    }

    #[test]
    fn lists_a_refine_in_flight_and_leaves_out_open_suggestions() {
        let state = begin_suggestion(&empty_suggestions(), ask("a", 0, 14, TargetSource::Block));
        let state = resolve_suggestion(&state, "a", &answer("One two."));
        assert!(working_jobs(&state, |_| None).is_empty());
        let state = begin_refine(&state, "a", free("warmer"));
        let jobs = working_jobs(&state, |_| None);
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            (jobs[0].kind, jobs[0].label.as_str()),
            (JobKind::Refine, "warmer")
        );
    }

    #[test]
    fn counts_a_reviews_suggestions_once_it_has_some() {
        assert_eq!(
            review_job(0, Duration::from_secs(3)).detail,
            "Whole text · 0:03"
        );
        assert_eq!(
            review_job(2, Duration::from_secs(3)).detail,
            "Whole text · 0:03 · 2 suggestions"
        );
    }

    #[test]
    fn an_answered_request_counts_its_changes_and_says_when_some_were_left_out() {
        let state = begin_suggestion(&empty_suggestions(), ask("a", 0, 14, TargetSource::Block));
        let state = resolve_suggestion(&state, "a", &answer("One two four."));
        let job = ready_job(&state, "a", false).unwrap();
        assert_eq!(job.phase, JobPhase::Ready);
        assert_eq!(job.detail, "Ready · 1 change");
        assert_eq!(
            ready_job(&state, "a", true).unwrap().detail,
            "Ready · 1 change, some left out"
        );
        assert_eq!(ready_job(&state, "gone", false), None);
        let refining = begin_refine(&state, "a", free("warmer"));
        assert_eq!(ready_job(&refining, "a", false), None);
    }
}

#[cfg(test)]
mod mark_spot_tests {
    use super::*;

    fn job(id: &str, phase: JobPhase) -> Job {
        Job {
            id: id.into(),
            kind: JobKind::Ask,
            phase,
            label: String::new(),
            detail: String::new(),
        }
    }

    #[test]
    fn a_new_mark_and_a_moved_one_are_news_and_a_still_one_is_not() {
        let mut spots = MarkSpots::default();
        assert!(spots.place("a", (37.5, 212.)));
        assert!(!spots.place("a", (37.5, 212.)));
        assert!(spots.place("a", (37.5, 240.)));
        assert_eq!(spots.iter().collect::<Vec<_>>(), [("a", &(37.5, 240.))]);
    }

    #[test]
    fn each_request_keeps_one_mark() {
        let mut spots = MarkSpots::default();
        spots.place("a", 1);
        spots.place("b", 2);
        spots.place("a", 3);
        assert_eq!(spots.iter().collect::<Vec<_>>(), [("a", &3), ("b", &2)]);
    }

    #[test]
    fn a_request_that_answers_or_goes_loses_its_mark() {
        let mut spots = MarkSpots::default();
        spots.place("ready", 1);
        spots.place("gone", 2);
        spots.place("working", 3);
        let jobs = [
            job("ready", JobPhase::Ready),
            job("working", JobPhase::Working),
        ];
        assert!(spots.keep_working(&jobs));
        assert_eq!(spots.iter().collect::<Vec<_>>(), [("working", &3)]);
        assert!(!spots.keep_working(&jobs));
    }
}
