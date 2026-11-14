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
    ui::{self, ButtonKind, ButtonSize},
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
    let base = div().flex_none();
    match mark_of(cause) {
        VersionMark::Ai => base.size(px(10.)).rounded_full().bg(roles.accent),
        VersionMark::You => base.size(px(8.)).rounded_full().bg(roles.ink),
        VersionMark::File => base.size(px(10.)).border_1().border_color(roles.faint),
        VersionMark::Restore => base
            .size(px(10.))
            .rounded_full()
            .border_1()
            .border_color(roles.ink),
    }
}

fn list_icon(roles: Roles) -> Div {
    let line = || {
        ui::row()
            .gap(px(2.))
            .child(div().size(px(2.)).bg(roles.ink))
            .child(div().w(px(8.)).h(px(1.5)).bg(roles.ink))
    };
    div()
        .size(px(14.))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(2.5))
        .child(line())
        .child(line())
        .child(line())
}

fn heading_scale(line: &str) -> Option<(f32, FontWeight)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    let spaced = line[level..].starts_with(char::is_whitespace);
    match (level, spaced) {
        (1, true) => Some((1.85, FontWeight::BOLD)),
        (2, true) => Some((1.5, FontWeight::SEMIBOLD)),
        (3, true) => Some((1.25, FontWeight::SEMIBOLD)),
        _ => None,
    }
}

fn styled_line(line: &str, base: Pixels) -> Div {
    match heading_scale(line) {
        Some((scale, weight)) => div().text_size(base * scale).font_weight(weight),
        None => div(),
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
                    color: Some(roles.ink),
                    strikethrough: Some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(roles.ink),
                    }),
                    ..Default::default()
                }),
            ),
            DiffHunk::Inserted { text, .. } => (
                text,
                Some(HighlightStyle {
                    color: Some(roles.ai),
                    background_color: Some(roles.raised),
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
        Some(DiffHunk::Equal { text, .. })
        | Some(DiffHunk::Deleted { text, .. })
        | Some(DiffHunk::Inserted { text, .. }) => text,
        None => "",
    }
}

pub(crate) fn line_diff(
    changes: &[LineChange],
    only_changed: bool,
    base: Pixels,
    roles: Roles,
) -> AnyElement {
    let mut rows: Vec<AnyElement> = Vec::new();
    for change in changes {
        match change {
            LineChange::Same { lines } if !only_changed => {
                rows.extend(lines.iter().map(|line| {
                    styled_line(line, base)
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
                styled_line(first_hunk_text(hunks), base)
                    .ml(px(-12.))
                    .mr(px(-12.))
                    .px(px(12.))
                    .border_l_2()
                    .border_color(roles.accent)
                    .bg(roles.fill)
                    .child(hunk_text(hunks, roles))
                    .into_any_element(),
            ),
        }
    }
    if rows.is_empty() {
        return div()
            .text_size(px(14.))
            .text_color(roles.faint)
            .child("No text changes.")
            .into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .text_size(base)
        .line_height(relative(1.75))
        .text_color(roles.ink)
        .when(only_changed, |this| this.gap(px(12.)))
        .children(rows)
        .into_any_element()
}

fn quiet_toggle(
    id: &'static str,
    label: &'static str,
    disabled: bool,
    pressed: bool,
    roles: Roles,
) -> Stateful<Div> {
    ui::sized_button(
        id,
        "",
        ButtonKind::Quiet,
        ButtonSize::Small,
        disabled,
        roles,
    )
    .gap(px(6.))
    .when(pressed, |this| this.bg(roles.fill).border_color(roles.ink))
    .child(label)
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
        self.history.text(id).ok().flatten()
    }

    pub(crate) fn render_timeline(
        &mut self,
        mode: HistoryMode,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let versions = self.history.snapshot().clone();
        let now = now_ms();
        let offset = self.utc_offset;
        let clock_of = move |at: i64| format_clock(at, now, offset);
        let selected = match mode {
            HistoryMode::Version { id } => Some(id),
            _ => None,
        };
        let gaps = versions.len().saturating_sub(1).max(1) as f32;
        let width = self.timeline_width.get();
        let dot_width = px(DOT_SIZE).min(width / gaps);
        let measured = self.timeline_width.clone();
        let dots = versions.iter().enumerate().map(|(index, version)| {
            let id = version.id;
            let share = index as f32 / gaps;
            let description = describe_version(&versions, version, clock_of);
            let clock = clock_of(version.at);
            let is_selected = selected == Some(id);
            let highlighted = self.hovered == Some(id);
            let tooltip = (self.dot_hovered == Some(id)).then(|| {
                let side = div().absolute().top(px(36.)).w(px(TOOLTIP_WIDTH));
                let side = if share < 0.25 {
                    side.left(-dot_width / 2.)
                } else if share > 0.75 {
                    side.right(-dot_width / 2.)
                } else {
                    side.left(px(-TOOLTIP_WIDTH / 2.))
                };
                deferred(
                    side.border_1()
                        .border_color(roles.hairline)
                        .bg(roles.raised)
                        .p(px(12.))
                        .text_size(px(12.))
                        .line_height(relative(1.4))
                        .text_color(roles.ink)
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(format!("{} · {clock}", description.label)),
                        )
                        .child(div().mt(px(4.)).child(description.headline.clone()))
                        .when_some(version.saved.clone(), |this, saved| {
                            this.child(
                                div()
                                    .mt(px(4.))
                                    .text_color(roles.faint)
                                    .child(format!("Saved to {saved}")),
                            )
                        }),
                )
                .with_priority(2)
            });
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
                    div()
                        .id(("timeline-dot", id as u64))
                        .flex_none()
                        .w(dot_width)
                        .h(px(DOT_SIZE))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .when(is_selected, |this| {
                            this.bg(roles.raised).border_1().border_color(roles.accent)
                        })
                        .when(highlighted && !is_selected, |this| this.bg(roles.raised))
                        .hover(|this| this.bg(roles.raised))
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
        });
        let track = div()
            .id("timeline")
            .relative()
            .h(px(32.))
            .flex_1()
            .mr(px(8.))
            .child(
                canvas(
                    move |bounds, _, _| measured.set(bounds.size.width),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(16.))
                    .h(px(1.))
                    .bg(roles.hairline),
            )
            .children(dots);
        column_row(
            ui::row()
                .gap(px(8.))
                .child(track)
                .child(
                    quiet_toggle(
                        "timeline-now",
                        "NOW",
                        mode == HistoryMode::Editing,
                        false,
                        roles,
                    )
                    .when(mode != HistoryMode::Editing, |this| {
                        this.on_click(
                            cx.listener(|this, _, window, cx| this.close_history(window, cx)),
                        )
                    }),
                )
                .child(
                    ui::sized_button(
                        "timeline-full",
                        "",
                        ButtonKind::Quiet,
                        ButtonSize::Small,
                        false,
                        roles,
                    )
                    .gap(px(6.))
                    .when(mode == HistoryMode::Full, |this| {
                        this.bg(roles.fill).border_color(roles.ink)
                    })
                    .child(list_icon(roles))
                    .child("FULL HISTORY")
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_full(window, cx))),
                ),
        )
        .into_any_element()
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
        let restorable = text.clone().filter(|_| !same);
        let at = version.at;
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
                    .py(px(10.))
                    .text_size(px(14.))
                    .text_color(roles.ink)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .child(div().font_weight(FontWeight::MEDIUM).child(format!(
                                        "{} · {}",
                                        self.clock(at, now),
                                        description.label
                                    )))
                                    .child(format!("\u{a0}· {}", description.headline)),
                            )
                            .child(div().text_size(px(12.)).child(note)),
                    )
                    .child(
                        ui::row()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(
                                ui::sized_button(
                                    "version-older",
                                    "‹ Older",
                                    ButtonKind::Default,
                                    ButtonSize::Regular,
                                    first,
                                    roles,
                                )
                                .when(!first, |this| {
                                    this.on_click(cx.listener(|this, _, window, cx| {
                                        this.step_version(-1, window, cx)
                                    }))
                                }),
                            )
                            .child(
                                ui::sized_button(
                                    "version-newer",
                                    "Newer ›",
                                    ButtonKind::Default,
                                    ButtonSize::Regular,
                                    last,
                                    roles,
                                )
                                .when(!last, |this| {
                                    this.on_click(cx.listener(|this, _, window, cx| {
                                        this.step_version(1, window, cx)
                                    }))
                                }),
                            )
                            .child(
                                ui::sized_button(
                                    "version-restore",
                                    "Restore",
                                    ButtonKind::Default,
                                    ButtonSize::Regular,
                                    restorable.is_none(),
                                    roles,
                                )
                                .when_some(
                                    restorable,
                                    |this, text| {
                                        this.on_click(cx.listener(move |this, _, window, cx| {
                                            this.restore(id, at, text.clone(), window, cx)
                                        }))
                                    },
                                ),
                            )
                            .child(
                                ui::button("version-back", "Back", ButtonKind::Default, roles)
                                    .child(
                                        div()
                                            .ml(px(8.))
                                            .text_size(px(12.))
                                            .font_weight(FontWeight::NORMAL)
                                            .child("Esc"),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_history(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element(),
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
            Some(changes) => line_diff(changes, false, px(17.), roles),
            None => div().into_any_element(),
        };
        div()
            .id("compare")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.compare_scroll)
            .child(centered(div().pt(px(16.)).pb(px(160.)).child(body)))
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
                .child(
                    div()
                        .font_family("League Spartan")
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(30.))
                        .text_color(roles.ink)
                        .child("How this Document changed"),
                )
                .child(div().mt(px(8.)).text_size(px(14.)).child(format!(
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
        div()
            .id("full-history")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.history_scroll)
            .child(header)
            .children(entries)
            .child(div().h(px(160.)))
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
        let label = format!(
            "{}{}",
            description.label,
            version
                .saved
                .as_ref()
                .map(|saved| format!(" · Saved to {saved}"))
                .unwrap_or_default()
        )
        .to_uppercase();
        let ai = matches!(description.author, VersionMark::Ai);
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
                .gap(px(20.))
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
                        .pt(px(2.))
                        .text_size(px(12.))
                        .text_color(roles.faint)
                        .child(format_clock(version.at, now, offset))
                        .child(format_ago(version.at, now)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .border_l_2()
                        .border_color(if ai { roles.accent } else { roles.fill_hover })
                        .pl(px(24.))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(roles.faint)
                                .child(label),
                        )
                        .child(
                            div()
                                .mt(px(6.))
                                .text_size(px(18.))
                                .text_color(roles.ink)
                                .child(description.headline.clone()),
                        )
                        .when(changes.len() > 1, |this| {
                            let mixed = mixed_labels(&changes);
                            this.child(
                                div()
                                    .mt(px(8.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .text_size(px(14.))
                                    .children(list_changes(&changes).into_iter().map(|listed| {
                                        ui::row()
                                            .flex_wrap()
                                            .when(mixed, |this| {
                                                this.child(
                                                    div()
                                                        .text_color(roles.faint)
                                                        .child(format!("{}:\u{a0}", listed.label)),
                                                )
                                            })
                                            .child(listed.rationale)
                                    })),
                            )
                        })
                        .when_some(diff, |this, diff| {
                            this.child(
                                div().mt(px(16.)).child(match diff {
                                    Some(changes) => line_diff(&changes, true, px(15.2), roles),
                                    None => div()
                                        .text_size(px(14.))
                                        .text_color(roles.faint)
                                        .child("This Version could not be loaded.")
                                        .into_any_element(),
                                }),
                            )
                        })
                        .child(ui::row().mt(px(12.)).h(px(30.)).when(hovered, |this| {
                            this.child(
                                ui::button(
                                    ("view-in-timeline", id as u64),
                                    "View in Timeline",
                                    ButtonKind::Default,
                                    roles,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| this.view_version(id, window, cx),
                                )),
                            )
                        })),
                ),
        )
        .into_any_element()
    }
}
