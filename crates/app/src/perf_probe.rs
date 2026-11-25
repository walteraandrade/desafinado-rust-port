//! Writes GPUI's foreground journal (input, draws, views, slow task polls) as
//! JSON lines to the file in `DESAFINADO_PERF_OUT`, for performance runs.
//! Built only with the `perf-probe` feature.

use std::{
    io::Write as _,
    path::PathBuf,
    sync::LazyLock,
    time::{Duration, Instant},
};

use desafinado_core::log_err::LogErr;
use gpui_kit::{
    App, AppContext as _,
    profiler::journal::{ForegroundEvent, ForegroundJournalEntry, IntervalBoundary},
};
use serde_json::json;

static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);

const DRAIN_EVERY: Duration = Duration::from_millis(250);

/// Marks the process start; call first thing in `main`.
pub fn mark_start() {
    LazyLock::force(&STARTED);
}

fn micros(at: Instant) -> u128 {
    at.saturating_duration_since(*STARTED).as_micros()
}

fn line(entry: &ForegroundJournalEntry) -> Option<serde_json::Value> {
    let event = match entry {
        ForegroundJournalEntry::Event(event) => event,
        ForegroundJournalEntry::Boundary(IntervalBoundary::Presented(frame)) => {
            return Some(json!({
                "k": "presented",
                "t": micros(frame.presentation.present_end),
                "dirty_to_present_us": frame.dirty_to_present_duration().map(|d| d.as_micros()),
            }));
        }
        _ => return None,
    };
    let (kind, name, extra) = match event {
        ForegroundEvent::Input(input) => (
            "input",
            input.kind.to_string(),
            json!({ "inval": input.caused_invalidation }),
        ),
        ForegroundEvent::Draw(frame) => (
            "draw",
            String::new(),
            json!({
                "layout_us": frame.phases.request_layout.as_micros(),
                "prepaint_us": frame.phases.prepaint.as_micros(),
                "paint_us": frame.phases.paint.as_micros(),
                "dirty_us": frame.dirty_to_draw_duration().map(|d| d.as_micros()),
            }),
        ),
        ForegroundEvent::View(view) => (
            "view",
            view.name.to_string(),
            json!({ "phase": view.phase.as_str(), "self_us": view.self_duration.as_micros() }),
        ),
        ForegroundEvent::TaskPoll(task) => (
            "task",
            format!("{}:{}", task.location.file(), task.location.line()),
            json!({}),
        ),
        ForegroundEvent::Action(action) => ("action", action.name.to_string(), json!({})),
        ForegroundEvent::Present(_) => ("present", String::new(), json!({})),
        ForegroundEvent::SmallPolls(flush) => (
            "small_polls",
            String::new(),
            json!({ "busy_us": flush.summary.total.as_micros(), "count": flush.summary.count }),
        ),
    };
    Some(json!({
        "k": kind,
        "name": name,
        "t": micros(event.start_time()),
        "us": event.duration().as_micros(),
        "x": extra,
    }))
}

static SPANS: std::sync::Mutex<Vec<(&'static str, Instant, Duration)>> =
    std::sync::Mutex::new(Vec::new());

/// Times the scope it lives in; see `perf_span!`.
pub struct Span(&'static str, Instant);

impl Span {
    pub fn new(name: &'static str) -> Self {
        Self(name, Instant::now())
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Ok(mut spans) = SPANS.lock() {
            spans.push((self.0, self.1, self.1.elapsed()));
        }
    }
}

fn span_lines() -> Vec<serde_json::Value> {
    let taken = SPANS
        .lock()
        .map(|mut spans| std::mem::take(&mut *spans))
        .unwrap_or_default();
    taken
        .into_iter()
        .map(|(name, start, took)| {
            json!({ "k": "span", "name": name, "t": micros(start), "us": took.as_micros() })
        })
        .collect()
}

/// Background task polls that ended since `seen`.
fn background_lines(
    foreground: std::thread::ThreadId,
    seen: &mut Instant,
) -> Vec<serde_json::Value> {
    let since = *seen;
    let mut lines = Vec::new();
    for thread in gpui_kit::profiler::get_all_timings(gpui_kit::TasksIncluded::OnlyCompleted) {
        if thread.thread_id == foreground {
            continue;
        }
        for task in thread.timings.iter().filter(|task| task.end.0 > since) {
            *seen = (*seen).max(task.end.0);
            lines.push(json!({
                "k": "bg",
                "name": format!("{}:{}", task.location.file(), task.location.line()),
                "t": micros(task.start),
                "us": task.end.0.saturating_duration_since(task.start).as_micros(),
            }));
        }
    }
    lines
}

/// Starts draining the journal into `DESAFINADO_PERF_OUT`, when it is set.
pub fn start(cx: &mut App) {
    let Some(path) = std::env::var_os("DESAFINADO_PERF_OUT").map(PathBuf::from) else {
        return;
    };
    gpui_kit::profiler::set_trace_enabled(true);
    let mut collector = cx.foreground_journal().collector();
    let foreground = std::thread::current().id();
    let mut seen = Instant::now();
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        loop {
            executor.timer(DRAIN_EVERY).await;
            let drained = collector.collect_unseen();
            let values: Vec<serde_json::Value> = drained
                .entries
                .iter()
                .filter_map(line)
                .chain(background_lines(foreground, &mut seen))
                .chain(span_lines())
                .collect();
            if values.is_empty() {
                continue;
            }
            let text = values.iter().fold(String::new(), |mut text, value| {
                text.push_str(&value.to_string());
                text.push('\n');
                text
            });
            let path = path.clone();
            cx.background_spawn(async move {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut file| file.write_all(text.as_bytes()))
                    .log_err();
            })
            .await;
        }
    })
    .detach();
}
