use gpui_kit::*;

use crate::theme::Roles;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Default,
    Strong,
    Quiet,
    Primary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonSize {
    Regular,
    Small,
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    roles: Roles,
) -> Stateful<Div> {
    sized_button(id, label, kind, ButtonSize::Regular, false, roles)
}

pub fn sized_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    size: ButtonSize,
    disabled: bool,
    roles: Roles,
) -> Stateful<Div> {
    let (height, padding, text) = match size {
        ButtonSize::Regular => (px(30.), px(12.), px(13.)),
        ButtonSize::Small => (px(24.), px(8.), px(12.)),
    };
    let base = div()
        .id(id)
        .h(height)
        .px(padding)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_size(text)
        .font_weight(FontWeight::MEDIUM)
        .border_1()
        .child(label.into());
    if disabled {
        return base
            .border_color(transparent_black())
            .text_color(roles.faint);
    }
    match kind {
        ButtonKind::Default => base
            .bg(roles.fill)
            .border_color(roles.fill)
            .text_color(roles.ink)
            .hover(|this| this.bg(roles.fill_hover).border_color(roles.fill_hover))
            .cursor_pointer(),
        ButtonKind::Strong => base
            .bg(roles.accent)
            .border_color(roles.accent)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(roles.canvas)
            .cursor_pointer(),
        ButtonKind::Quiet => base
            .border_color(transparent_black())
            .text_color(roles.muted)
            .hover(|this| this.bg(roles.fill).text_color(roles.ink))
            .cursor_pointer(),
        ButtonKind::Primary => base
            .bg(roles.ai)
            .border_color(roles.ai)
            .text_color(roles.canvas)
            .cursor_pointer(),
    }
}

pub fn dialog_frame(id: impl Into<ElementId>, width: Pixels, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .occlude()
        .w(width)
        .max_w_full()
        .p(px(24.))
        .bg(roles.overlay)
        .border_1()
        .border_color(roles.overlay)
        .text_color(roles.ink)
        .flex()
        .flex_col()
}

pub fn modal_backdrop(
    id: impl Into<ElementId>,
    roles: Roles,
    content: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id)
        .occlude()
        .absolute()
        .inset_0()
        .bg(roles.canvas)
        .flex()
        .items_center()
        .justify_center()
        .child(content)
}

pub fn heading(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .text_size(px(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(roles.heading)
        .child(text.into())
}

pub fn detail(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .mt(px(8.))
        .text_size(px(14.))
        .text_color(roles.ink)
        .child(text.into())
}

pub fn row() -> Div {
    div().flex().flex_row().items_center()
}

pub fn actions_row() -> Div {
    row().mt(px(20.)).justify_end().gap(px(8.))
}

pub fn ai_flag(roles: Roles) -> Div {
    div()
        .px(px(4.))
        .h(px(16.))
        .flex()
        .items_center()
        .bg(roles.ai)
        .text_color(roles.canvas)
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .child("AI")
}

pub fn field(input: impl IntoElement, focused: bool, invalid: bool, roles: Roles) -> Div {
    div()
        .w_full()
        .px(px(10.))
        .py(px(3.))
        .bg(if focused {
            roles.fill_hover
        } else {
            roles.fill
        })
        .border_1()
        .border_color(if invalid {
            roles.danger
        } else if focused {
            roles.accent
        } else {
            roles.fill
        })
        .text_size(px(14.))
        .text_color(roles.ink)
        .child(input)
}

pub fn label(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .mb(px(4.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(roles.muted)
        .child(text.into())
}

pub fn problem_list(problems: Vec<String>, roles: Roles) -> Div {
    div()
        .mt(px(4.))
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(12.))
        .text_color(roles.danger)
        .children(problems.into_iter().map(|problem| div().child(problem)))
}
