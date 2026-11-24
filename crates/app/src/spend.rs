use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};
use desafinado_ai::records::{AiCallRecord, CallOutcome, CostBucket, summarize_costs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Today,
    Week,
    Month,
    All,
}

pub const PERIODS: [Period; 4] = [Period::Today, Period::Week, Period::Month, Period::All];

impl Period {
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Week => "7 days",
            Self::Month => "30 days",
            Self::All => "All time",
        }
    }
}

pub fn started_at(record: &AiCallRecord) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&record.started_at)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

fn local_date(at: DateTime<Utc>, offset: FixedOffset) -> NaiveDate {
    at.with_timezone(&offset).date_naive()
}

pub fn in_period(
    record: &AiCallRecord,
    period: Period,
    now: DateTime<Utc>,
    offset: FixedOffset,
) -> bool {
    let Some(at) = started_at(record) else {
        return period == Period::All;
    };
    match period {
        Period::All => true,
        Period::Today => local_date(at, offset) == local_date(now, offset),
        Period::Week => at > now - Duration::days(7),
        Period::Month => at > now - Duration::days(30),
    }
}

pub fn records_in(
    records: &[AiCallRecord],
    period: Period,
    now: DateTime<Utc>,
    offset: FixedOffset,
) -> Vec<AiCallRecord> {
    records
        .iter()
        .filter(|record| in_period(record, period, now, offset))
        .cloned()
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpendRow {
    pub label: String,
    pub bucket: CostBucket,
    pub share: f64,
}

fn rows_of(buckets: impl Iterator<Item = (String, CostBucket)>, total: f64) -> Vec<SpendRow> {
    let mut rows: Vec<SpendRow> = buckets
        .map(|(label, bucket)| SpendRow {
            share: if total > 0.0 {
                bucket.cost_usd / total
            } else {
                0.0
            },
            label,
            bucket,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.bucket
            .cost_usd
            .total_cmp(&a.bucket.cost_usd)
            .then(b.bucket.calls.cmp(&a.bucket.calls))
            .then(a.label.cmp(&b.label))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
pub struct DaySpend {
    pub date: NaiveDate,
    pub calls: usize,
    pub cost_usd: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpendReport {
    pub total: CostBucket,
    pub failed: usize,
    pub retried: usize,
    pub by_model: Vec<SpendRow>,
    pub by_task: Vec<SpendRow>,
    pub days: Vec<DaySpend>,
}

pub fn task_group(task: &str) -> String {
    let task = task.trim();
    if task.starts_with("review document") {
        return "Review".into();
    }
    if task.starts_with("refine ") {
        return "Refine".into();
    }
    match task.split_once(" → ") {
        Some((_, what)) if what.starts_with("free text:") => "Ask AI".into(),
        Some((_, what)) => what.trim().to_string(),
        None => task.to_string(),
    }
}

pub fn days_of(
    records: &[AiCallRecord],
    now: DateTime<Utc>,
    offset: FixedOffset,
    count: i64,
) -> Vec<DaySpend> {
    let today = local_date(now, offset);
    (0..count)
        .rev()
        .map(|back| {
            let date = today - Duration::days(back);
            let of_day: Vec<&AiCallRecord> = records
                .iter()
                .filter(|record| {
                    started_at(record).is_some_and(|at| local_date(at, offset) == date)
                })
                .collect();
            DaySpend {
                date,
                calls: of_day.len(),
                cost_usd: of_day.iter().filter_map(|record| record.cost_usd).sum(),
            }
        })
        .collect()
}

pub fn spend_report(
    records: &[AiCallRecord],
    now: DateTime<Utc>,
    offset: FixedOffset,
) -> SpendReport {
    let summary = summarize_costs(records);
    let total = summary.total.cost_usd;
    let grouped: Vec<AiCallRecord> = records
        .iter()
        .map(|record| AiCallRecord {
            task: task_group(&record.task),
            ..record.clone()
        })
        .collect();
    let tasks = summarize_costs(&grouped).by_task;
    SpendReport {
        failed: records
            .iter()
            .filter(|record| record.outcome != CallOutcome::Valid)
            .count(),
        retried: records.iter().filter(|record| record.attempt > 1).count(),
        by_model: rows_of(summary.by_model.into_iter(), total),
        by_task: rows_of(tasks.into_iter(), total),
        days: days_of(records, now, offset, 14),
        total: summary.total,
    }
}

pub fn recent(records: &[AiCallRecord], limit: usize) -> Vec<AiCallRecord> {
    let mut sorted: Vec<AiCallRecord> = records.to_vec();
    sorted.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    sorted.truncate(limit);
    sorted
}

pub fn format_usd(cost: f64) -> String {
    if cost == 0.0 {
        "US$0.00".into()
    } else if cost >= 1.0 {
        format!("US${cost:.2}")
    } else if cost >= 0.01 {
        format!("US${cost:.4}")
    } else {
        format!("US${cost:.6}")
    }
}

pub fn format_count(value: f64) -> String {
    let whole = value.round() as i64;
    let digits = whole.abs().to_string();
    let grouped = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(",");
    if whole < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

pub fn format_latency(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

pub fn average_cost(bucket: &CostBucket) -> Option<f64> {
    (bucket.priced_calls > 0).then(|| bucket.cost_usd / bucket.priced_calls as f64)
}

pub fn served_by(record: &AiCallRecord) -> String {
    let model = record
        .served_model
        .clone()
        .unwrap_or_else(|| record.requested_model.clone());
    match &record.provider {
        Some(provider) => format!("{model} · {provider}"),
        None => model,
    }
}

pub fn finish_of(record: &AiCallRecord) -> String {
    let retry = if record.attempt > 1 { " (retry)" } else { "" };
    match record.outcome {
        CallOutcome::Valid => format!(
            "{}{retry}",
            record.finish_reason.clone().unwrap_or_else(|| "–".into())
        ),
        CallOutcome::Invalid => format!("invalid{retry}"),
        CallOutcome::Error => format!("error{retry}"),
    }
}

pub fn when_of(record: &AiCallRecord, now: DateTime<Utc>, offset: FixedOffset) -> String {
    let Some(at) = started_at(record) else {
        return record.started_at.clone();
    };
    let local = at.with_timezone(&offset);
    if local.date_naive() == local_date(now, offset) {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %-d, %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_ai::records::{LatencyMs, Tokens};

    fn record(at: &str, task: &str, model: &str, cost: Option<f64>) -> AiCallRecord {
        AiCallRecord {
            started_at: at.into(),
            task: task.into(),
            attempt: 1,
            outcome: CallOutcome::Valid,
            requested_model: "openai/gpt-6.1-terra".into(),
            served_model: Some(model.into()),
            provider: Some("OpenAI".into()),
            tokens: Tokens {
                input: Some(515.0),
                output: Some(82.0),
                reasoning: None,
                cached: None,
            },
            cost_usd: cost,
            latency_ms: LatencyMs {
                client: 1850,
                open_router: None,
                generation: None,
                first_element: None,
                first_suggestion: None,
            },
            finish_reason: Some("stop".into()),
            generation_id: None,
            error: None,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-14T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    #[test]
    fn filters_by_period() {
        let records = vec![
            record("2026-11-14T09:00:00Z", "a", "m", Some(0.001)),
            record("2026-11-10T09:00:00Z", "a", "m", Some(0.002)),
            record("2026-10-20T09:00:00Z", "a", "m", Some(0.004)),
            record("2025-01-01T09:00:00Z", "a", "m", Some(0.008)),
        ];
        let count = |period| records_in(&records, period, now(), utc()).len();
        assert_eq!(count(Period::Today), 1);
        assert_eq!(count(Period::Week), 2);
        assert_eq!(count(Period::Month), 3);
        assert_eq!(count(Period::All), 4);
    }

    #[test]
    fn today_follows_the_local_day() {
        let late = record("2026-11-14T01:00:00Z", "a", "m", None);
        let brasilia = FixedOffset::west_opt(3 * 3600).unwrap();
        assert!(!in_period(&late, Period::Today, now(), brasilia));
        assert!(in_period(&late, Period::Today, now(), utc()));
    }

    #[test]
    fn groups_tasks_by_their_instruction_and_sorts_by_cost() {
        let records = vec![
            record(
                "2026-11-14T09:00:00Z",
                "select paragraph 2 → Shorter",
                "m1",
                Some(0.000_925),
            ),
            record(
                "2026-11-14T09:01:00Z",
                "block 3 → Shorter",
                "m1",
                Some(0.000_925),
            ),
            record(
                "2026-11-14T09:02:00Z",
                "review document → default criteria",
                "m2",
                Some(0.00258),
            ),
            record(
                "2026-11-14T09:03:00Z",
                "review document → focus: tone",
                "m2",
                None,
            ),
        ];
        let report = spend_report(&records, now(), utc());
        assert_eq!(report.total.calls, 4);
        assert_eq!(report.total.priced_calls, 3);
        assert_eq!(report.by_task[0].label, "Review");
        assert_eq!(report.by_task[0].bucket.calls, 2);
        assert_eq!(report.by_task[1].label, "Shorter");
        assert_eq!(report.by_task[1].bucket.calls, 2);
        assert_eq!(report.by_model[0].label, "m2");
        let share: f64 = report.by_model.iter().map(|row| row.share).sum();
        assert!((share - 1.0).abs() < 1e-9);
    }

    #[test]
    fn names_each_kind_of_task() {
        assert_eq!(task_group("select p3 → Shorter"), "Shorter");
        assert_eq!(task_group("cursor in p5 → free text: friendlier"), "Ask AI");
        assert_eq!(task_group("refine p3 → make it shorter"), "Refine");
        assert_eq!(task_group("review document → focus: tone"), "Review");
        assert_eq!(
            task_group("Improve skill description"),
            "Improve skill description"
        );
    }

    #[test]
    fn counts_failures_and_retries() {
        let failed = AiCallRecord {
            outcome: CallOutcome::Error,
            attempt: 2,
            ..record("2026-11-14T09:00:00Z", "a", "m", None)
        };
        let report = spend_report(std::slice::from_ref(&failed), now(), utc());
        assert_eq!(report.failed, 1);
        assert_eq!(report.retried, 1);
        assert_eq!(finish_of(&failed), "error (retry)");
    }

    #[test]
    fn lists_fourteen_days_ending_today() {
        let records = vec![
            record("2026-11-14T09:00:00Z", "a", "m", Some(0.5)),
            record("2026-11-14T10:00:00Z", "a", "m", Some(0.25)),
            record("2026-11-01T09:00:00Z", "a", "m", Some(1.0)),
        ];
        let days = days_of(&records, now(), utc(), 14);
        assert_eq!(days.len(), 14);
        assert_eq!(days[13].date.to_string(), "2026-11-14");
        assert_eq!(days[13].calls, 2);
        assert!((days[13].cost_usd - 0.75).abs() < 1e-9);
        assert_eq!(days[0].date.to_string(), "2026-11-01");
        assert_eq!(days[0].calls, 1);
    }

    #[test]
    fn shows_the_newest_calls_first() {
        let records = vec![
            record("2026-11-14T09:00:00Z", "old", "m", None),
            record("2026-11-14T10:00:00Z", "new", "m", None),
        ];
        let shown = recent(&records, 1);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].task, "new");
    }

    #[test]
    fn formats_money_counts_and_latency() {
        assert_eq!(format_usd(0.0), "US$0.00");
        assert_eq!(format_usd(0.000_925), "US$0.000925");
        assert_eq!(format_usd(0.0258), "US$0.0258");
        assert_eq!(format_usd(12.3456), "US$12.35");
        assert_eq!(format_count(1_234_567.0), "1,234,567");
        assert_eq!(format_count(82.0), "82");
        assert_eq!(format_latency(950), "950 ms");
        assert_eq!(format_latency(1850), "1.9 s");
    }

    #[test]
    fn names_who_served_a_call_and_when() {
        let one = record(
            "2026-11-14T09:05:00Z",
            "a",
            "openai/gpt-6.1-terra-20261026",
            None,
        );
        assert_eq!(served_by(&one), "openai/gpt-6.1-terra-20261026 · OpenAI");
        assert_eq!(when_of(&one, now(), utc()), "09:05");
        let older = record("2026-11-02T09:05:00Z", "a", "m", None);
        assert_eq!(when_of(&older, now(), utc()), "Nov 2, 09:05");
        assert_eq!(
            average_cost(&spend_report(&[one], now(), utc()).total),
            None
        );
    }
}
