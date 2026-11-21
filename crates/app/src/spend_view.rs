use std::sync::Arc;

use chrono::{FixedOffset, Utc};
use desafinado_ai::records::AiCallRecord;
use desafinado_runtime::services::Services;
use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::{
    spend::{
        PERIODS, Period, SpendReport, SpendRow, average_cost, finish_of, format_count,
        format_latency, format_usd, recent, records_in, served_by, spend_report, when_of,
    },
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize, Type as _},
    workspace::utc_offset_minutes,
};

pub const RECENT_LIMIT: usize = 50;
const SPEND_WIDTH: f32 = 1040.;

pub enum SpendEvent {
    Close,
}

impl EventEmitter<SpendEvent> for SpendView {}

enum Loaded {
    Loading,
    Ready(Vec<AiCallRecord>),
    Failed(String),
}

pub struct SpendView {
    services: Arc<Services>,
    loaded: Loaded,
    period: Period,
    scroll: ScrollHandle,
    _task: Option<Task<()>>,
}

impl SpendView {
    pub fn new(services: Arc<Services>, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            services,
            loaded: Loaded::Loading,
            period: Period::All,
            scroll: ScrollHandle::new(),
            _task: None,
        };
        view.reload(cx);
        view
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let work = cx.background_spawn(async move { services.log.read_all() });
        self._task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.loaded = match result {
                    Ok(records) => Loaded::Ready(records),
                    Err(error) => {
                        Loaded::Failed(format!("Could not read the AI call records: {error}"))
                    }
                };
                cx.notify();
            });
        }));
    }
}

fn offset() -> FixedOffset {
    FixedOffset::east_opt((utc_offset_minutes() * 60) as i32)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero offset"))
}

fn section_title(text: &'static str, roles: Roles) -> Div {
    ui::title(text, roles).mt(px(28.)).mb(px(8.))
}

fn tile(label: &'static str, value: String, note: Option<String>, roles: Roles) -> Div {
    div()
        .flex_1()
        .min_w(px(140.))
        .p(px(12.))
        .bg(roles.raised)
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(ui::meta(label, roles))
        .child(div().weight_strong().text_color(roles.heading).child(value))
        .children(note.map(|note| ui::meta(note, roles)))
}

#[derive(Clone, Copy)]
struct Column {
    width: Option<f32>,
    right: bool,
}

const fn grow() -> Column {
    Column {
        width: None,
        right: false,
    }
}

const fn fixed(width: f32, right: bool) -> Column {
    Column {
        width: Some(width),
        right,
    }
}

fn table_row(cells: Vec<String>, columns: &[Column], header: bool, roles: Roles) -> Div {
    ui::row()
        .w_full()
        .min_h(px(32.))
        .gap(px(12.))
        .px(px(8.))
        .map(|this| {
            if header {
                this.type_small().text_color(roles.muted)
            } else {
                this.bg(roles.raised)
            }
        })
        .children(cells.into_iter().zip(columns.iter()).map(|(text, column)| {
            let cell = if column.right {
                div().flex().justify_end().whitespace_nowrap().child(text)
            } else {
                div().truncate().child(text)
            };
            match column.width {
                Some(width) => cell.w(px(width)).flex_none(),
                None => cell.flex_1().min_w_0(),
            }
        }))
}

fn table(header: Div, rows: impl IntoIterator<Item = Div>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(header)
        .children(rows)
}

fn share_bar(id: SharedString, index: usize, share: f64, roles: Roles) -> Div {
    div()
        .w(px(80.))
        .h(px(8.))
        .flex_none()
        .bg(roles.fill)
        .child(ui::grow_width(
            id,
            div().h_full().bg(roles.accent),
            px((80. * share.clamp(0., 1.)) as f32),
            index,
        ))
}

fn tokens(input: Option<f64>, output: Option<f64>) -> String {
    let count = |value: Option<f64>| value.map(format_count).unwrap_or_else(|| "–".into());
    format!("{} / {}", count(input), count(output))
}

fn bucket_table(title_column: &'static str, period: &str, rows: &[SpendRow], roles: Roles) -> Div {
    let columns = [
        grow(),
        fixed(56., true),
        fixed(120., true),
        fixed(96., true),
        fixed(80., false),
    ];
    let header = table_row(
        vec![
            title_column.into(),
            "Calls".into(),
            "Tokens in / out".into(),
            "Cost".into(),
            "Share".into(),
        ],
        &columns,
        true,
        roles,
    );
    table(
        header,
        rows.iter().enumerate().map(|(index, row)| {
            let bucket = &row.bucket;
            let cells = vec![
                row.label.clone(),
                bucket.calls.to_string(),
                if bucket.input_tokens + bucket.output_tokens > 0.0 {
                    tokens(Some(bucket.input_tokens), Some(bucket.output_tokens))
                } else {
                    "–".into()
                },
                if bucket.priced_calls > 0 {
                    format_usd(bucket.cost_usd)
                } else {
                    "–".into()
                },
            ];
            let id = SharedString::from(format!("share-{title_column}-{period}-{index}"));
            table_row(cells, &columns[..4], false, roles)
                .child(share_bar(id, index, row.share, roles))
        }),
    )
}

fn recent_table(
    shown: &[AiCallRecord],
    now: chrono::DateTime<Utc>,
    offset: FixedOffset,
    roles: Roles,
) -> Div {
    let columns = [
        fixed(92., false),
        grow(),
        fixed(210., false),
        fixed(100., true),
        fixed(96., true),
        fixed(60., true),
        fixed(96., false),
    ];
    let header = table_row(
        [
            "When",
            "Task",
            "Served by",
            "Tokens in / out",
            "Cost",
            "Latency",
            "Finish",
        ]
        .map(String::from)
        .to_vec(),
        &columns,
        true,
        roles,
    );
    table(
        header,
        recent(shown, RECENT_LIMIT).iter().map(|record| {
            table_row(
                vec![
                    when_of(record, now, offset),
                    record.task.clone(),
                    served_by(record),
                    tokens(record.tokens.input, record.tokens.output),
                    record
                        .cost_usd
                        .map(format_usd)
                        .unwrap_or_else(|| "–".into()),
                    format_latency(record.latency_ms.client),
                    finish_of(record),
                ],
                &columns,
                false,
                roles,
            )
            .when(record.error.is_some(), |this| this.text_color(roles.danger))
        }),
    )
}

fn day_chart(report: &SpendReport, roles: Roles) -> Div {
    let peak = report
        .days
        .iter()
        .map(|day| day.cost_usd)
        .fold(0.0_f64, f64::max);
    let height = 96.0_f64;
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            ui::row()
                .items_end()
                .h(px(height as f32))
                .gap(px(4.))
                .children(report.days.iter().enumerate().map(|(index, day)| {
                    let share = if peak > 0.0 { day.cost_usd / peak } else { 0.0 };
                    let bar = (height * share).max(if day.calls > 0 { 2.0 } else { 0.0 });
                    let tip = format!(
                        "{}: {} in {} {}",
                        day.date.format("%b %-d"),
                        format_usd(day.cost_usd),
                        day.calls,
                        if day.calls == 1 { "call" } else { "calls" }
                    );
                    ui::chart_slot(("spend-day", index), roles)
                        .flex_1()
                        .h_full()
                        .flex()
                        .flex_col()
                        .justify_end()
                        .tooltip(move |window, cx| {
                            component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        })
                        .child(ui::grow_height(
                            ("spend-day-bar", index),
                            div().w_full().bg(roles.accent),
                            px(bar as f32),
                            index,
                        ))
                })),
        )
        .child(ui::axis(roles))
        .child(
            ui::row()
                .gap(px(4.))
                .type_small()
                .text_color(roles.muted)
                .children(report.days.iter().enumerate().map(|(index, day)| {
                    div().flex_1().flex().justify_center().child(
                        if index % 2 == 1 || index + 1 == report.days.len() {
                            day.date.format("%-d").to_string()
                        } else {
                            String::new()
                        },
                    )
                })),
        )
}

fn tiles(report: &SpendReport, roles: Roles) -> Div {
    let total = &report.total;
    let unpriced = total.calls - total.priced_calls;
    ui::row()
        .mt(px(16.))
        .flex_wrap()
        .gap(px(8.))
        .child(tile(
            "Total",
            format_usd(total.cost_usd),
            Some(if unpriced == 0 {
                "Every call is priced".into()
            } else {
                format!(
                    "{unpriced} {} without a price",
                    if unpriced == 1 { "call" } else { "calls" }
                )
            }),
            roles,
        ))
        .child(tile(
            "Calls",
            total.calls.to_string(),
            Some(format!("{} priced", total.priced_calls)),
            roles,
        ))
        .child(tile(
            "Per call",
            average_cost(total)
                .map(format_usd)
                .unwrap_or_else(|| "–".into()),
            Some("Average of priced calls".into()),
            roles,
        ))
        .child(tile(
            "Tokens",
            tokens(Some(total.input_tokens), Some(total.output_tokens)),
            Some(format!(
                "In / out · {} reasoning",
                format_count(total.reasoning_tokens)
            )),
            roles,
        ))
        .child(tile(
            "Failed",
            report.failed.to_string(),
            Some(format!("{} retried", report.retried)),
            roles,
        ))
}

impl SpendView {
    fn render_periods(&self, roles: Roles, cx: &mut Context<Self>) -> Div {
        ui::row().gap(px(4.)).children(PERIODS.iter().map(|period| {
            let period = *period;
            ui::toggle_button(
                SharedString::from(format!("spend-period-{}", period.label())),
                period.label(),
                period == self.period,
                ui::Control::default(),
                roles,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.period = period;
                cx.notify();
            }))
        }))
    }

    fn render_ready(&self, records: &[AiCallRecord], roles: Roles, cx: &mut Context<Self>) -> Div {
        if records.is_empty() {
            return div().mt(px(24.)).child(
                "No AI calls yet. Each call the AI makes for you is listed here with its cost.",
            );
        }
        let now = Utc::now();
        let offset = offset();
        let shown = records_in(records, self.period, now, offset);
        let report = spend_report(&shown, now, offset);
        let all_days = spend_report(records, now, offset);
        div()
            .child(div().mt(px(20.)).child(self.render_periods(roles, cx)))
            .child(tiles(&report, roles))
            .child(section_title("Last 14 days", roles))
            .child(day_chart(&all_days, roles))
            .child(section_title("By model", roles))
            .child(bucket_table(
                "Model served",
                self.period.label(),
                &report.by_model,
                roles,
            ))
            .child(section_title("By task", roles))
            .child(bucket_table(
                "Task",
                self.period.label(),
                &report.by_task,
                roles,
            ))
            .child(section_title("Recent calls", roles))
            .child(recent_table(&shown, now, offset, roles))
            .when(shown.len() > RECENT_LIMIT, |this| {
                this.child(
                    ui::meta(
                        format!(
                            "The newest {RECENT_LIMIT} of {} calls. Every call is in {}.",
                            shown.len(),
                            self.services.log.path().display()
                        ),
                        roles,
                    )
                    .mt(px(8.)),
                )
            })
    }
}

impl Render for SpendView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let body = match &self.loaded {
            Loaded::Loading => div().mt(px(24.)).child("Reading the AI call records…"),
            Loaded::Failed(message) => div()
                .mt(px(24.))
                .text_color(roles.danger)
                .child(message.clone()),
            Loaded::Ready(records) => {
                let records = records.clone();
                self.render_ready(&records, roles, cx)
            }
        };
        let header = ui::row()
            .pt(px(8.))
            .gap(px(12.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(ui::title("AI spend", roles))
                    .child(ui::detail(
                        "What the AI cost on this machine, from its call records. OpenRouter prices each call after it ends.",
                        roles,
                    ).mt(px(4.))),
            )
            .child(
                ui::sized_button(
                    "spend-refresh",
                    "Refresh",
                    ButtonKind::Default,
                    ButtonSize::Small, ui::Control::default(),
                    roles,)
                .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
            .child(
                ui::sized_button(
                    "spend-close",
                    "Done",
                    ButtonKind::Default,
                    ButtonSize::Small, ui::Control::default(),
                    roles,)
                .on_click(cx.listener(|_, _, _, cx| cx.emit(SpendEvent::Close))),
            );
        let page = div()
            .id("spend-page")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .type_body()
            .text_color(roles.ink)
            .child(
                div().w_full().flex().justify_center().child(ui::enter(
                    "spend-in",
                    div()
                        .w(px(SPEND_WIDTH))
                        .max_w_full()
                        .px(px(24.))
                        .pb(px(160.))
                        .child(header)
                        .child(body),
                    ui::Enter::Rise(6.),
                    ui::MOTION_ENTER,
                )),
            );
        ui::scrolled(page, &self.scroll)
    }
}
