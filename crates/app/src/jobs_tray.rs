//! The requests tray in the editor's bottom right corner: each request the model works on, with
//! its time and a way to cancel it, and each request that has just answered, with a way to go to
//! its Suggestion. The text, the caret and the selection stay free while requests run.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use desafinado_ai::editor::jobs::{
    Job, JobKind, JobPhase, READY_ROW_TIME, ready_job, review_job, working_jobs,
};
use desafinado_core::{
    log_err::LogErr,
    suggestions::{NoticeKind, SuggestionsState, cancel_refine},
};
use gpui_kit::*;

use crate::{
    editor_pane::EditorPane,
    theme::Roles,
    ui::{self, ButtonKind},
};

const TRAY_WIDTH: f32 = 304.;
const TRAY_INSET: f32 = 16.;
/// High enough to clear a toast at the bottom of the window.
const TRAY_BOTTOM: f32 = 64.;
const TICK: Duration = Duration::from_secs(1);

/// When each request left, the requests that answered lately, and the clock that redraws the
/// tray while any of them shows.
#[derive(Default)]
pub struct JobsState {
    started: HashMap<String, Instant>,
    answered: Vec<Answered>,
    pub(crate) review_started: Option<Instant>,
    tick: Option<Task<()>>,
}

struct Answered {
    id: String,
    left_out: bool,
    at: Instant,
}

impl JobsState {
    pub fn started(&mut self, id: &str) {
        self.started.insert(id.to_string(), Instant::now());
    }

    /// Forgets the times of requests that are no longer in flight.
    pub fn keep_started(&mut self, live: &[String]) {
        self.started.retain(|id, _| live.contains(id));
    }

    /// Notes a request that has just answered, so the tray shows it for a while. `next` is the
    /// state with its answer; a notice that some changes were left out marks the row.
    pub fn answered(&mut self, id: &str, before: &SuggestionsState, next: &SuggestionsState) {
        let left_out = next.notice.is_some_and(|notice| {
            notice.kind == NoticeKind::PartlyKept
                && before.notice.map(|old| old.serial) != Some(notice.serial)
        });
        self.answered.retain(|row| row.id != id);
        self.answered.push(Answered {
            id: id.to_string(),
            left_out,
            at: Instant::now(),
        });
    }

    fn forget_answer(&mut self, id: &str) {
        self.answered.retain(|row| row.id != id);
    }

    pub fn clear(&mut self) {
        self.started.clear();
        self.answered.clear();
        self.review_started = None;
    }
}

impl EditorPane {
    /// The tray's rows: Reviews first, then requests in flight, then those that just answered,
    /// except the one whose card is open.
    fn jobs(&mut self, cx: &App) -> Vec<Job> {
        let now = Instant::now();
        let suggestions = &self.suggestions;
        self.jobs.answered.retain(|row| {
            now.duration_since(row.at) < READY_ROW_TIME
                && ready_job(suggestions, &row.id, false).is_some()
        });
        let review = self.busy().map(|added| {
            let since = self
                .jobs
                .review_started
                .map_or(Duration::ZERO, |at| now - at);
            review_job(added, since)
        });
        let text = self.text(cx);
        let started = &self.jobs.started;
        let working = working_jobs(suggestions, &text, |id| {
            started.get(id).map(|at| now.duration_since(*at))
        });
        let shown = self.active(cx).map(|active| active.id.clone());
        let ready = self
            .jobs
            .answered
            .iter()
            .filter(|row| Some(&row.id) != shown.as_ref())
            .filter_map(|row| ready_job(suggestions, &row.id, row.left_out));
        review.into_iter().chain(working).chain(ready).collect()
    }

    /// Redraws the tray every second while it shows anything, for the times and the rows that
    /// leave.
    pub(crate) fn keep_tray_ticking(&mut self, cx: &mut Context<Self>) {
        if self.jobs.tick.is_some() {
            return;
        }
        self.jobs.tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let going = this.update(cx, |this, cx| {
                    let going = !this.jobs(cx).is_empty();
                    if !going {
                        this.jobs.tick = None;
                    }
                    cx.notify();
                    going
                });
                if !matches!(going, Ok(true)) {
                    going.log_warn();
                    break;
                }
            }
        }));
    }

    fn cancel_job(&mut self, job: &Job, cx: &mut Context<Self>) {
        match job.kind {
            JobKind::Review => self.stop_review(cx),
            JobKind::Ask => self.reject(&job.id, cx),
            JobKind::Refine => {
                let next = cancel_refine(&self.suggestions, &job.id);
                self.set_suggestions(next, cx);
            }
        }
    }

    fn open_job(&mut self, job: &Job, window: &mut Window, cx: &mut Context<Self>) {
        if job.kind == JobKind::Review {
            return;
        }
        self.jobs.forget_answer(&job.id);
        self.show(&job.id, window, cx);
    }

    pub(crate) fn render_jobs_tray(
        &mut self,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let jobs = self.jobs(cx);
        if jobs.is_empty() {
            return None;
        }
        self.keep_tray_ticking(cx);
        let right = self.map_left().map_or(px(TRAY_INSET), |left| {
            self.pane_bounds().size.width - left + px(TRAY_INSET / 2.)
        });
        let rows = jobs
            .into_iter()
            .enumerate()
            .map(|(index, job)| self.render_job(index, job, roles, cx));
        Some(
            ui::jobs_tray("jobs-tray", px(TRAY_WIDTH), roles)
                .absolute()
                .right(right)
                .bottom(px(TRAY_BOTTOM))
                .children(rows)
                .into_any_element(),
        )
    }

    fn render_job(
        &self,
        index: usize,
        job: Job,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let working = job.phase == JobPhase::Working;
        let action = match (job.kind, job.phase) {
            (JobKind::Review, _) => "Stop",
            (_, JobPhase::Working) => "Cancel",
            (_, JobPhase::Ready) => "Show",
        };
        let opened = job.clone();
        let acted = job.clone();
        let button = ui::small_button(("job-action", index), action, ButtonKind::Default, roles)
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                if acted.phase == JobPhase::Working {
                    this.cancel_job(&acted, cx);
                } else {
                    this.open_job(&acted, window, cx);
                }
                cx.notify();
            }));
        ui::job_row(("job", index), job.kind != JobKind::Review, roles)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_job(&opened, window, cx);
                cx.notify();
            }))
            .child(ui::job_mark(("job-mark", index), working, roles))
            .child(ui::job_text(job.label, job.detail, roles))
            .child(button)
            .into_any_element()
    }
}
