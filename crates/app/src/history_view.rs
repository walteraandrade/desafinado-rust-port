use desafinado_core::log_err::LogErr;
use std::rc::Rc;

use desafinado_features::{
    diff::DiffHunk,
    history::{
        line_changes::{LineChange, line_changes},
        story::{
            VersionMark, describe_version, history_summary, list_changes, mark_of, mixed_labels,
            plural,
        },
        time::{format_ago, format_clock},
        types::{VersionCause, VersionId, VersionSummary},
        view_mode::HistoryMode,
    },
};
use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::{
    theme::Roles,
    ui::{self, ButtonKind, ButtonSize, Pip, Type as _},
    workspace::{Workspace, column_row, now_ms},
};

const DOT_SIZE: f32 = 24.;
const TOOLTIP_WIDTH: f32 = 256.;
const BLANK_LINE: &str = "\u{a0}";

fn shows_diff(version: &VersionSummary) -> bool {
    !matches!(
        version.cause,
        VersionCause::Started | VersionCause::Opened { .. }
    )
}

fn mark(cause: &VersionCause, roles: Roles) -> Div {
    ui::pip(
        match mark_of(cause) {
            VersionMark::Ai => Pip::Ai,
            VersionMark::You => Pip::You,
            VersionMark::File => Pip::File,
            VersionMark::Restore => Pip::Restore,
        },
        roles,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EvalsButton {
    pub pressed: bool,
    pub busy: bool,
}

fn is_heading(line: &str) -> bool {
    let level = line.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&level) && line[level..].starts_with(char::is_whitespace)
}

fn styled_line(line: &str, roles: Roles) -> Div {
    if is_heading(line) {
        div().weight_bold().text_color(roles.heading)
    } else {
        div()
    }
}

fn hunk_text(hunks: &[DiffHunk], roles: Roles) -> StyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for hunk in hunks {
        let (piece, style) = match hunk {
            DiffHunk::Equal { text, .. } => (text, None),
            DiffHunk::Deleted { text, .. } => (
                text,
                Some(HighlightStyle {
                    color: Some(roles.muted),
                    strikethrough: Some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(roles.muted),
                    }),
                    ..Default::default()
                }),
            ),
            DiffHunk::Inserted { text, .. } => (
                text,
                Some(HighlightStyle {
                    color: Some(roles.ai),
                    background_color: Some(roles.ai_fill),
                    ..Default::default()
                }),
            ),
        };
        let start = text.len();
        text.push_str(piece);
        if let Some(style) = style {
            highlights.push((start..text.len(), style));
        }
    }
    if text.is_empty() {
        text.push_str(BLANK_LINE);
    }
    StyledText::new(text).with_highlights(highlights)
}

fn first_hunk_text(hunks: &[DiffHunk]) -> &str {
    match hunks.first() {
        Some(
            DiffHunk::Equal { text, .. }
            | DiffHunk::Deleted { text, .. }
            | DiffHunk::Inserted { text, .. },
        ) => text,
        None => "",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffSize {
    Document,
    Body,
}

pub(crate) fn line_diff(
    changes: &[LineChange],
    only_changed: bool,
    size: DiffSize,
    roles: Roles,
) -> AnyElement {
    let mut rows: Vec<AnyElement> = Vec::new();
    for change in changes {
        match change {
            LineChange::Same { lines } if !only_changed => {
                rows.extend(lines.iter().map(|line| {
                    styled_line(line, roles)
                        .child(if line.is_empty() {
                            BLANK_LINE.to_string()
                        } else {
                            line.clone()
                        })
                        .into_any_element()
                }));
            }
            LineChange::Same { .. } => {}
            LineChange::Changed { hunks } => rows.push(
                styled_line(first_hunk_text(hunks), roles)
                    .ml(px(-12.))
                    .mr(px(-12.))
                    .px(px(12.))
                    .bg(roles.fill)
                    .child(hunk_text(hunks, roles))
                    .into_any_element(),
            ),
        }
    }
    if rows.is_empty() {
        return ui::meta("No text changes.", roles).into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .map(|this| match size {
            DiffSize::Document => this.type_document(),
            DiffSize::Body => this.type_body(),
        })
        .line_height(relative(1.75))
        .text_color(roles.ink)
        .when(only_changed, |this| this.gap(px(12.)))
        .children(rows)
        .into_any_element()
}

fn centered(content: impl IntoElement) -> Div {
    div()
        .w_full()
        .flex()
        .justify_center()
        .child(div().w(px(768.)).max_w_full().px(px(24.)).child(content))
}

impl Workspace {
    fn clock(&self, at: i64, now: i64) -> String {
        format_clock(at, now, self.utc_offset)
    }

    fn version_text(&mut self, id: VersionId) -> Option<String> {
        self.history.text(id).log_err().flatten()
    }

    pub(crate) fn render_timeline(
        &mut self,
        mode: HistoryMode,
        evals: Option<EvalsButton>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let evals_pressed = evals.is_some_and(|evals| evals.pressed);
        let editing = mode == HistoryMode::Editing && !evals_pressed;
        let track = self.render_track(mode, roles, cx);
        column_row(
            ui::row()
                .gap(px(8.))
                .child(track)
                .child(
                    ui::toggle_button(
                        "timeline-now",
                        "Now",
                        false,
                        ui::Control::disabled(editing),
                        roles,
                    )
                    .tooltip(|window, cx| {
                        component::tooltip::Tooltip::new("Back to the current text (Esc)")
                            .build(window, cx)
                    })
                    .when(!editing, |this| {
                        this.on_click(
                            cx.listener(|this, _, window, cx| this.close_history(window, cx)),
                        )
                    }),
                )
                .child(
                    ui::toggle_button(
                        "timeline-full",
                        "Full history",
                        mode == HistoryMode::Full,
                        ui::Control::default(),
                        roles,
                    )
                    .child(ui::list_icon(roles))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_full(window, cx))),
                )
                .when_some(evals, |this, evals| {
                    this.child(
                        ui::toggle_button(
                            "timeline-evals",
                            "Evals",
                            evals.pressed,
                            ui::Control::default(),
                            roles,
                        )
                        .tooltip(|window, cx| {
                            component::tooltip::Tooltip::new("How this Skill performs")
                                .build(window, cx)
                        })
                        .child(ui::gauge_icon(evals.busy, roles))
                        .on_click(cx.listener(|this, _, window, cx| this.toggle_evals(window, cx))),
                    )
                }),
        )
        .into_any_element()
    }

    fn render_track(
        &self,
        mode: HistoryMode,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let versions = self.history.snapshot().clone();
        let selected = match mode {
            HistoryMode::Version { id } => Some(id),
            _ => None,
        };
        let gaps = versions.len().saturating_sub(1).max(1) as f32;
        let dot_width = px(DOT_SIZE).min(self.timeline_width.get() / gaps);
        let measured = Rc::clone(&self.timeline_width);
        let dots: Vec<Div> = versions
            .iter()
            .enumerate()
            .map(|(index, version)| {
                let id = version.id;
                let share = index as f32 / gaps;
                let tooltip = (self.dot_hovered == Some(id))
                    .then(|| self.render_dot_tooltip(&versions, version, share, dot_width, roles));
                div()
                    .absolute()
                    .top_0()
                    .h_full()
                    .left(relative(share))
                    .w(px(0.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        ui::dot_button(
                            ("timeline-dot", id as u64),
                            dot_width,
                            selected == Some(id),
                            self.hovered == Some(id),
                            roles,
                        )
                        .child(mark(&version.cause, roles))
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered {
                                this.dot_hovered = Some(id);
                            } else if this.dot_hovered == Some(id) {
                                this.dot_hovered = None;
                            }
                            cx.notify();
                        }))
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.on_dot(id, window, cx)),
                        ),
                    )
                    .children(tooltip)
            })
            .collect();
        div()
            .id("timeline")
            .relative()
            .h(px(32.))
            .flex_1()
            .mr(px(8.))
            .child(
                canvas(
                    move |bounds, _, _| measured.set(bounds.size.width),
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(ui::axis(roles).absolute().left_0().right_0().top(px(16.)))
            .children(dots)
    }

    fn render_dot_tooltip(
        &self,
        versions: &[VersionSummary],
        version: &VersionSummary,
        share: f32,
        dot_width: Pixels,
        roles: Roles,
    ) -> Deferred {
        let now = now_ms();
        let offset = self.utc_offset;
        let clock_of = move |at: i64| format_clock(at, now, offset);
        let description = describe_version(versions, version, clock_of);
        let side = div().absolute().top(px(36.)).w(px(TOOLTIP_WIDTH));
        let side = if share < 0.25 {
            side.left(-dot_width / 2.)
        } else if share > 0.75 {
            side.right(-dot_width / 2.)
        } else {
            side.left(px(-TOOLTIP_WIDTH / 2.))
        };
        deferred(
            side.bg(roles.overlay)
                .p(px(12.))
                .type_small()
                .line_height(relative(1.4))
                .text_color(roles.ink)
                .child(
                    div()
                        .weight_medium()
                        .text_color(roles.heading)
                        .child(format!("{} · {}", description.label, clock_of(version.at))),
                )
                .child(div().mt(px(4.)).child(description.headline.clone()))
                .when_some(version.saved.clone(), |this, saved| {
                    this.child(ui::meta(format!("Saved to {saved}"), roles).mt(px(4.)))
                }),
        )
        .with_priority(2)
    }

    pub(crate) fn render_version_bar(
        &mut self,
        mode: HistoryMode,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let HistoryMode::Version { id } = mode else {
            return None;
        };
        let versions = self.history.snapshot().clone();
        let version = versions.iter().find(|version| version.id == id)?.clone();
        let text = self.version_text(id);
        let current = self.text(cx);
        let now = now_ms();
        let offset = self.utc_offset;
        let description = describe_version(&versions, &version, |at| format_clock(at, now, offset));
        let same = text.as_deref() == Some(current.as_str());
        let note = match &text {
            None => "This Version could not be loaded.",
            Some(_) if same => "Same text as now.",
            Some(_) => "Marks show what Restore changes in the current text.",
        };
        let first = versions.first().map(|v| v.id) == Some(id);
        let last = versions.last().map(|v| v.id) == Some(id);
        let restorable = text.filter(|_| !same);
        let at = version.at;
        let summary = div()
            .min_w_0()
            .flex_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .child(
                        div()
                            .weight_medium()
                            .text_color(roles.heading)
                            .child(format!("{} · {}", self.clock(at, now), description.label)),
                    )
                    .child(format!("\u{a0}· {}", description.headline)),
            )
            .child(ui::meta(note, roles));
        Some(
            column_row(
                div()
                    .id("version-bar")
                    .w_full()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_x(px(16.))
                    .gap_y(px(8.))
                    .bg(roles.raised)
                    .px(px(16.))
                    .py(px(12.))
                    .type_body()
                    .text_color(roles.ink)
                    .child(summary)
                    .child(self.render_version_actions(id, at, first, last, restorable, roles, cx)),
            )
            .into_any_element(),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is one piece of the row's state; grouping them would only rename them"
    )]
    fn render_version_actions(
        &self,
        id: VersionId,
        at: i64,
        first: bool,
        last: bool,
        restorable: Option<String>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let step = |button_id: &'static str, label: &'static str, edge: bool, delta: i64| {
            ui::sized_button(
                button_id,
                label,
                ButtonKind::Default,
                ButtonSize::Regular,
                ui::Control::disabled(edge),
                roles,
            )
            .when(!edge, |this| {
                this.on_click(
                    cx.listener(move |this, _, window, cx| this.step_version(delta, window, cx)),
                )
            })
        };
        ui::row()
            .flex_wrap()
            .gap(px(8.))
            .child(step("version-older", "‹ Older", first, -1))
            .child(step("version-newer", "Newer ›", last, 1))
            .child(
                ui::sized_button(
                    "version-restore",
                    "Restore",
                    ButtonKind::Strong,
                    ButtonSize::Regular,
                    ui::Control::disabled(restorable.is_none()),
                    roles,
                )
                .when_some(restorable, |this, text| {
                    this.on_click(cx.listener(move |this, _, window, cx| {
                        this.restore(id, at, text.clone(), window, cx);
                    }))
                }),
            )
            .child(
                ui::button("version-back", "Back", ButtonKind::Default, roles)
                    .child(ui::shortcut_hint("Esc", false, roles))
                    .on_click(cx.listener(|this, _, window, cx| this.close_history(window, cx))),
            )
    }

    fn step_version(&mut self, delta: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_mode(
            desafinado_features::history::view_mode::HistoryModeAction::Step { delta },
            window,
            cx,
        );
        self.compare_scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(crate) fn render_compare(
        &mut self,
        id: VersionId,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.text(cx);
        let fresh =
            matches!(&self.compare, Some((known, text, _)) if *known == id && *text == current);
        if !fresh {
            let changes = self
                .version_text(id)
                .map(|shown| line_changes(&current, &shown));
            self.compare = Some((id, current, changes));
        }
        let body = match self
            .compare
            .as_ref()
            .and_then(|(_, _, changes)| changes.as_ref())
        {
            Some(changes) => line_diff(changes, false, DiffSize::Document, roles),
            None => div().into_any_element(),
        };
        ui::scrolled(
            div()
                .id("compare")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.compare_scroll)
                .child(centered(div().pt(px(16.)).pb(px(160.)).child(body))),
            &self.compare_scroll,
        )
        .into_any_element()
    }

    fn entry_diff(&mut self, before: VersionId, after: VersionId) -> Option<Vec<LineChange>> {
        if let Some(known) = self.entry_diffs.get(&(before, after)) {
            return known.clone();
        }
        let changes = match (self.version_text(before), self.version_text(after)) {
            (Some(previous), Some(next)) => Some(line_changes(&previous, &next)),
            _ => None,
        };
        self.entry_diffs.insert((before, after), changes.clone());
        changes
    }

    pub(crate) fn render_full_history(
        &mut self,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let versions = self.history.snapshot().clone();
        let now = now_ms();
        let offset = self.utc_offset;
        let summary = history_summary(&versions);
        let since = versions
            .first()
            .map(|first| format!(" since {}", format_clock(first.at, now, offset)))
            .unwrap_or_default();
        let header = centered(
            div()
                .pt(px(16.))
                .pb(px(40.))
                .child(ui::title("How this Document changed", roles))
                .child(div().mt(px(8.)).text_color(roles.muted).child(format!(
                    "{}{since} · {} from AI · {} by you",
                    plural(summary.versions, "Version"),
                    summary.from_ai,
                    summary.by_you
                ))),
        );
        let entries: Vec<AnyElement> = versions
            .iter()
            .enumerate()
            .rev()
            .map(|(index, version)| {
                let previous = index.checked_sub(1).map(|at| versions[at].clone());
                self.render_entry(&versions, version, previous, now, roles, cx)
                    .into_any_element()
            })
            .collect();
        ui::scrolled(
            div()
                .id("full-history")
                .type_body()
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.history_scroll)
                .child(header)
                .children(entries)
                .child(div().h(px(160.))),
            &self.history_scroll,
        )
        .into_any_element()
    }

    fn render_entry(
        &mut self,
        versions: &[VersionSummary],
        version: &VersionSummary,
        previous: Option<VersionSummary>,
        now: i64,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = version.id;
        let offset = self.utc_offset;
        let description = describe_version(versions, version, |at| format_clock(at, now, offset));
        let changes = match &version.cause {
            VersionCause::Accepted { changes } => changes.clone(),
            _ => Vec::new(),
        };
        let diff = previous
            .filter(|_| shows_diff(version))
            .map(|previous| self.entry_diff(previous.id, id));
        let flashing = self.flash.as_ref().is_some_and(|(flash, _)| *flash == id);
        let hovered = self.hovered == Some(id);
        let saved = version
            .saved
            .as_ref()
            .map(|saved| format!(" · Saved to {saved}"))
            .unwrap_or_default();
        let ai = matches!(description.author, VersionMark::Ai);
        let body = div()
            .flex_1()
            .min_w_0()
            .child(
                ui::meta(format!("{}{saved}", description.label), roles)
                    .weight_medium()
                    .when(ai, |this| this.text_color(roles.ai)),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_color(roles.ink)
                    .child(description.headline.clone()),
            )
            .when(changes.len() > 1, |this| {
                this.child(entry_changes(&changes, roles))
            })
            .when_some(diff, |this, diff| {
                this.child(div().mt(px(16.)).child(match diff {
                    Some(changes) => line_diff(&changes, true, DiffSize::Body, roles),
                    None => ui::meta("This Version could not be loaded.", roles).into_any_element(),
                }))
            })
            .child(ui::row().mt(px(12.)).h(px(32.)).when(hovered, |this| {
                this.child(
                    ui::button(
                        ("view-in-timeline", id as u64),
                        "View in Timeline",
                        ButtonKind::Default,
                        roles,
                    )
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.view_version(id, window, cx)),
                    ),
                )
            }));
        centered(
            div()
                .id(("history-entry", id as u64))
                .ml(px(-16.))
                .mr(px(-16.))
                .mb(px(32.))
                .px(px(16.))
                .py(px(12.))
                .flex()
                .flex_row()
                .gap(px(24.))
                .when(flashing, |this| this.bg(roles.fill))
                .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                    if *over {
                        this.hovered = Some(id);
                    } else if this.hovered == Some(id) {
                        this.hovered = None;
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(80.))
                        .flex_none()
                        .type_small()
                        .text_color(roles.muted)
                        .child(format_clock(version.at, now, offset))
                        .child(format_ago(version.at, now)),
                )
                .child(body),
        )
        .into_any_element()
    }
}

fn entry_changes(
    changes: &[desafinado_features::history::types::AcceptedChange],
    roles: Roles,
) -> Div {
    let mixed = mixed_labels(changes);
    div()
        .mt(px(8.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .children(list_changes(changes).into_iter().map(|listed| {
            ui::row()
                .flex_wrap()
                .when(mixed, |this| {
                    this.child(
                        div()
                            .text_color(roles.muted)
                            .child(format!("{}:\u{a0}", listed.label)),
                    )
                })
                .child(listed.rationale)
        }))
}
