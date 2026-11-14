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
    ui::{self, ButtonKind, ButtonSize},
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
    div()
        .mt(px(28.))
        .mb(px(8.))
        .text_size(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(roles.ink)
        .child(text)
}

fn tile(label: &'static str, value: String, note: Option<String>, roles: Roles) -> Div {
    div()
        .flex_1()
        .min_w(px(140.))
        .p(px(12.))
        .bg(roles.raised)
        .border_1()
        .border_color(roles.hairline)
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().text_size(px(12.)).child(label))
        .child(
            div()
                .text_size(px(22.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(value),
        )
        .children(note.map(|note| div().text_size(px(12.)).text_color(roles.faint).child(note)))
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
        .min_h(px(30.))
        .gap(px(12.))
        .px(px(8.))
        .border_b_1()
        .border_color(roles.hairline)
        .text_size(px(if header { 12. } else { 13. }))
        .when(header, |this| this.font_weight(FontWeight::SEMIBOLD))
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

fn share_bar(share: f64, roles: Roles) -> Div {
    div()
        .w(px(80.))
        .h(px(8.))
        .flex_none()
        .bg(roles.fill)
        .border_1()
        .border_color(roles.hairline)
        .child(
            div()
                .h_full()
                .w(px((78. * share.clamp(0., 1.)) as f32))
                .bg(roles.accent),
        )
}

fn bucket_table(title_column: &'static str, rows: &[SpendRow], roles: Roles) -> Div {
    let columns = [
        grow(),
        fixed(56., true),
        fixed(120., true),
        fixed(96., true),
        fixed(80., false),
    ];
    div()
        .border_t_1()
        .border_color(roles.hairline)
        .child(table_row(
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
        ))
        .children(rows.iter().map(|row| {
            let priced = row.bucket.priced_calls > 0;
            let cells = vec![
                row.label.clone(),
                row.bucket.calls.to_string(),
                if row.bucket.input_tokens + row.bucket.output_tokens > 0.0 {
                    format!(
                        "{} / {}",
                        format_count(row.bucket.input_tokens),
                        format_count(row.bucket.output_tokens)
                    )
                } else {
                    "–".into()
                },
                if priced {
                    format_usd(row.bucket.cost_usd)
                } else {
                    "–".into()
                },
            ];
            table_row(cells, &columns[..4], false, roles).child(share_bar(row.share, roles))
        }))
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
                .border_b_1()
                .border_color(roles.hairline)
                .children(report.days.iter().enumerate().map(|(index, day)| {
                    let share = if peak > 0.0 { day.cost_usd / peak } else { 0.0 };
                    let bar = ((height - 2.0) * share).max(if day.calls > 0 { 2.0 } else { 0.0 });
                    let tip = format!(
                        "{}: {} in {} {}",
                        day.date.format("%b %-d"),
                        format_usd(day.cost_usd),
                        day.calls,
                        if day.calls == 1 { "call" } else { "calls" }
                    );
                    div()
                        .id(("spend-day", index))
                        .flex_1()
                        .h_full()
                        .flex()
                        .flex_col()
                        .justify_end()
                        .hover(|this| this.bg(roles.fill))
                        .tooltip(move |window, cx| {
                            component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        })
                        .child(div().w_full().h(px(bar as f32)).bg(roles.accent))
                })),
        )
        .child(
            ui::row()
                .gap(px(4.))
                .text_size(px(11.))
                .text_color(roles.faint)
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

impl SpendView {
    fn render_ready(&self, records: &[AiCallRecord], roles: Roles, cx: &mut Context<Self>) -> Div {
        let now = Utc::now();
        let offset = offset();
        let shown = records_in(records, self.period, now, offset);
        let report = spend_report(&shown, now, offset);
        let all_days = spend_report(records, now, offset);
        let total = &report.total;
        let periods = ui::row().gap(px(4.)).children(PERIODS.iter().map(|period| {
            let period = *period;
            let chosen = period == self.period;
            ui::sized_button(
                SharedString::from(format!("spend-period-{}", period.label())),
                period.label(),
                ButtonKind::Quiet,
                ButtonSize::Small,
                false,
                roles,
            )
            .when(chosen, |this| {
                this.bg(roles.fill).border_color(roles.accent)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.period = period;
                cx.notify();
            }))
        }));
        if records.is_empty() {
            return div().child(div().mt(px(24.)).text_size(px(14.)).child(
                "No AI calls yet. Each call the AI makes for you is listed here with its cost.",
            ));
        }
        let unpriced = total.calls - total.priced_calls;
        let recent_columns = [
            fixed(92., false),
            grow(),
            fixed(210., false),
            fixed(100., true),
            fixed(96., true),
            fixed(60., true),
            fixed(96., false),
        ];
        div()
            .child(div().mt(px(20.)).child(periods))
            .child(
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
                        format!(
                            "{} / {}",
                            format_count(total.input_tokens),
                            format_count(total.output_tokens)
                        ),
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
                    )),
            )
            .child(section_title("Last 14 days", roles))
            .child(day_chart(&all_days, roles))
            .child(section_title("By model", roles))
            .child(bucket_table("Model served", &report.by_model, roles))
            .child(section_title("By task", roles))
            .child(bucket_table("Task", &report.by_task, roles))
            .child(section_title("Recent calls", roles))
            .child(
                div()
                    .border_t_1()
                    .border_color(roles.hairline)
                    .child(table_row(
                        vec![
                            "When".into(),
                            "Task".into(),
                            "Served by".into(),
                            "Tokens in / out".into(),
                            "Cost".into(),
                            "Latency".into(),
                            "Finish".into(),
                        ],
                        &recent_columns,
                        true,
                        roles,
                    ))
                    .children(recent(&shown, RECENT_LIMIT).iter().map(|record| {
                        let failed = record.error.is_some();
                        table_row(
                            vec![
                                when_of(record, now, offset),
                                record.task.clone(),
                                served_by(record),
                                format!(
                                    "{} / {}",
                                    record
                                        .tokens
                                        .input
                                        .map(format_count)
                                        .unwrap_or_else(|| "–".into()),
                                    record
                                        .tokens
                                        .output
                                        .map(format_count)
                                        .unwrap_or_else(|| "–".into())
                                ),
                                record
                                    .cost_usd
                                    .map(format_usd)
                                    .unwrap_or_else(|| "–".into()),
                                format_latency(record.latency_ms.client),
                                finish_of(record),
                            ],
                            &recent_columns,
                            false,
                            roles,
                        )
                        .when(failed, |this| this.text_color(roles.danger))
                    })),
            )
            .when(shown.len() > RECENT_LIMIT, |this| {
                this.child(
                    div()
                        .mt(px(8.))
                        .text_size(px(12.))
                        .text_color(roles.faint)
                        .child(format!(
                            "The newest {RECENT_LIMIT} of {} calls. Every call is in {}.",
                            shown.len(),
                            self.services.log.path().display()
                        )),
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
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("AI spend"),
                    )
                    .child(div().mt(px(4.)).text_size(px(13.)).child(
                        "What the AI cost on this machine, from its call records. OpenRouter prices each call after it ends.",
                    )),
            )
            .child(
                ui::sized_button(
                    "spend-refresh",
                    "Refresh",
                    ButtonKind::Default,
                    ButtonSize::Small,
                    false,
                    roles,
                )
                .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
            .child(
                ui::sized_button(
                    "spend-close",
                    "Done",
                    ButtonKind::Default,
                    ButtonSize::Small,
                    false,
                    roles,
                )
                .on_click(cx.listener(|_, _, _, cx| cx.emit(SpendEvent::Close))),
            );
        div()
            .id("spend-page")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .text_size(px(14.))
            .text_color(roles.ink)
            .child(
                div().w_full().flex().justify_center().child(
                    div()
                        .w(px(SPEND_WIDTH))
                        .max_w_full()
                        .px(px(24.))
                        .pb(px(160.))
                        .child(header)
                        .child(body),
                ),
            )
    }
}
