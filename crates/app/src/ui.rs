//! The shared components. Every size, weight, border, hover and click
//! affordance of the UI is decided here, so the screens only arrange them.
//! Hierarchy is carried by color: two text sizes, and borders only for the
//! keyboard focus ring, an invalid field and the empty data marks.

use std::rc::Rc;

use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::theme::Roles;

pub const TEXT_SMALL: Pixels = px(12.);
pub const TEXT_BODY: Pixels = px(14.);
/// The Document's own text, in the editor and wherever a Version is read.
pub const TEXT_DOCUMENT: Pixels = px(17.);
pub const MONO_FAMILY: &str = "DejaVu Sans Mono";
const WORDMARK_FAMILY: &str = "League Spartan";

pub trait Type: Styled + Sized {
    fn type_body(self) -> Self {
        self.text_size(TEXT_BODY)
    }

    fn type_document(self) -> Self {
        self.text_size(TEXT_DOCUMENT)
    }

    fn type_small(self) -> Self {
        self.text_size(TEXT_SMALL)
    }

    fn weight_regular(self) -> Self {
        self.font_weight(FontWeight::NORMAL)
    }

    fn weight_medium(self) -> Self {
        self.font_weight(FontWeight::MEDIUM)
    }

    fn weight_strong(self) -> Self {
        self.font_weight(FontWeight::SEMIBOLD)
    }

    fn weight_bold(self) -> Self {
        self.font_weight(FontWeight::BOLD)
    }

    fn type_mono(self) -> Self {
        self.font_family(MONO_FAMILY)
    }
}

impl<T: Styled> Type for T {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Default,
    Strong,
    Quiet,
    Primary,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonSize {
    Regular,
    Small,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Control {
    pub disabled: bool,
    pub focused: bool,
}

impl Control {
    pub fn disabled(disabled: bool) -> Self {
        Self {
            disabled,
            focused: false,
        }
    }

    pub fn focused(focused: bool) -> Self {
        Self {
            disabled: false,
            focused,
        }
    }
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    roles: Roles,
) -> Stateful<Div> {
    sized_button(
        id,
        label,
        kind,
        ButtonSize::Regular,
        Control::default(),
        roles,
    )
}

pub fn small_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    roles: Roles,
) -> Stateful<Div> {
    sized_button(
        id,
        label,
        kind,
        ButtonSize::Small,
        Control::default(),
        roles,
    )
}

pub fn sized_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    size: ButtonSize,
    control: Control,
    roles: Roles,
) -> Stateful<Div> {
    let (height, padding) = match size {
        ButtonSize::Regular => (px(32.), px(12.)),
        ButtonSize::Small => (px(24.), px(8.)),
    };
    let base = div()
        .id(id)
        .h(height)
        .px(padding)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .when(size == ButtonSize::Regular, |this| this.type_body())
        .when(size == ButtonSize::Small, |this| this.type_small())
        .weight_medium()
        .border_2()
        .when(control.focused, |this| this.border_color(roles.accent))
        .child(label.into());
    if control.disabled {
        return base.text_color(roles.faint);
    }
    let (background, text, hover) = match kind {
        ButtonKind::Default => (Some(roles.fill), roles.ink, roles.fill_hover),
        ButtonKind::Strong => (Some(roles.accent), roles.canvas, roles.accent_hover),
        ButtonKind::Primary => (Some(roles.ai), roles.canvas, roles.ai_hover),
        ButtonKind::Quiet => (None, roles.muted, roles.fill),
        ButtonKind::Danger => (Some(roles.fill), roles.danger, roles.danger_fill),
    };
    base.when_some(background, |this, background| this.bg(background))
        .when(kind == ButtonKind::Strong, |this| this.weight_strong())
        .text_color(text)
        .cursor_pointer()
        .hover(move |this| {
            let this = this.bg(hover);
            if kind == ButtonKind::Quiet {
                this.text_color(roles.ink)
            } else {
                this
            }
        })
}

pub fn toggle_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    pressed: bool,
    control: Control,
    roles: Roles,
) -> Stateful<Div> {
    sized_button(
        id,
        label,
        ButtonKind::Quiet,
        ButtonSize::Small,
        control,
        roles,
    )
    .when(pressed && !control.disabled, |this| {
        this.bg(roles.selection).text_color(roles.heading)
    })
}

pub fn shortcut_hint(text: impl Into<SharedString>, on_fill: bool, roles: Roles) -> Div {
    div()
        .type_small()
        .weight_regular()
        .text_color(if on_fill { roles.canvas } else { roles.muted })
        .child(text.into())
}

pub fn icon_button(
    id: impl Into<ElementId>,
    icon: component::IconName,
    pressed: bool,
    roles: Roles,
) -> Stateful<Div> {
    let color = if pressed { roles.accent } else { roles.ink };
    div()
        .id(id)
        .size(px(32.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .bg(if pressed { roles.selection } else { roles.fill })
        .text_color(color)
        .cursor_pointer()
        .hover(|this| this.bg(roles.fill_hover))
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(component::Icon::new(icon).size(px(16.)).text_color(color))
}

pub fn menu_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    open: bool,
    roles: Roles,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(4.))
        .type_body()
        .text_color(roles.ink)
        .when(open, |this| this.bg(roles.fill))
        .cursor_pointer()
        .hover(|this| this.bg(roles.fill))
        .child(label.into())
}

/// A row in a list, menu or picker. `active` is the keyboard or pointer
/// highlight; `chosen` marks the current value.
pub fn option_row(
    id: impl Into<ElementId>,
    active: bool,
    chosen: bool,
    roles: Roles,
) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .text_color(if chosen { roles.heading } else { roles.ink })
        .when(chosen, |this| this.bg(roles.selection))
        .when(active && !chosen, |this| this.bg(roles.fill))
        .cursor_pointer()
        .hover(move |this| this.bg(if chosen { roles.selection } else { roles.fill }))
}

/// A row of the ask bubble: the highlight is the AI's color.
pub fn ai_option_row(id: impl Into<ElementId>, active: bool, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .type_body()
        .bg(if active { roles.ai_fill } else { roles.fill })
        .text_color(if active { roles.ai } else { roles.ink })
        .cursor_pointer()
}

/// An option row that can hold the keyboard focus, shown by the focus ring.
pub fn focusable_row(
    id: impl Into<ElementId>,
    focused: bool,
    chosen: bool,
    roles: Roles,
) -> Stateful<Div> {
    option_row(id, false, chosen, roles)
        .border_2()
        .when(focused, |this| this.border_color(roles.accent))
}

pub fn disabled_row(id: impl Into<ElementId>, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .text_color(roles.faint)
}

/// A checkbox mark. The empty box is an outline, the filled one the accent.
pub fn check(checked: bool, roles: Roles) -> Div {
    div()
        .size(px(12.))
        .flex_none()
        .border_1()
        .border_color(if checked { roles.accent } else { roles.muted })
        .when(checked, |this| this.bg(roles.accent))
}

/// A whole surface that acts on click, such as a status chip.
pub fn chip(id: impl Into<ElementId>, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(24.))
        .px(px(8.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .bg(roles.fill)
        .type_small()
        .weight_medium()
        .cursor_pointer()
        .hover(|this| this.bg(roles.fill_hover))
}

pub fn link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    roles: Roles,
) -> Stateful<Div> {
    div()
        .id(id)
        .text_color(roles.accent)
        .weight_medium()
        .cursor_pointer()
        .hover(|this| this.underline())
        .child(label.into())
}

/// A round marker on the History timeline.
pub fn dot_button(
    id: impl Into<ElementId>,
    width: Pixels,
    selected: bool,
    highlighted: bool,
    roles: Roles,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .w(width)
        .h(px(24.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |this| this.bg(roles.selection))
        .when(highlighted && !selected, |this| this.bg(roles.fill))
        .cursor_pointer()
        .hover(move |this| {
            this.bg(if selected {
                roles.selection
            } else {
                roles.fill
            })
        })
}

/// Covers the window to catch the click that closes a menu.
pub fn scrim(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).occlude().absolute().inset_0()
}

pub fn dialog_frame(id: impl Into<ElementId>, width: Pixels, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .occlude()
        .w(width)
        .max_w_full()
        .p(px(24.))
        .bg(roles.overlay)
        .text_color(roles.ink)
        .type_body()
        .flex()
        .flex_col()
}

pub fn popover_frame(id: impl Into<ElementId>, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .occlude()
        .bg(roles.overlay)
        .text_color(roles.ink)
        .type_body()
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
        .p(px(16.))
        .bg(roles.canvas)
        .flex()
        .items_center()
        .justify_center()
        .child(content)
}

pub fn wordmark(roles: Roles) -> Div {
    div()
        .flex_none()
        .font_family(WORDMARK_FAMILY)
        .weight_strong()
        .text_size(px(24.))
        .text_color(roles.heading)
        .child("Desafinado")
}

/// A dialog, panel or page title: the body size, set apart by color.
pub fn title(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .type_body()
        .weight_strong()
        .text_color(roles.heading)
        .child(text.into())
}

pub fn detail(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .mt(px(8.))
        .type_body()
        .text_color(roles.ink)
        .child(text.into())
}

/// Secondary text: dates, counts, hints, captions.
pub fn meta(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .type_small()
        .text_color(roles.muted)
        .child(text.into())
}

pub fn label(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .mb(px(4.))
        .type_small()
        .weight_medium()
        .text_color(roles.muted)
        .child(text.into())
}

pub fn error_text(text: impl Into<SharedString>, roles: Roles) -> Div {
    div().text_color(roles.danger).child(text.into())
}

pub fn row() -> Div {
    div().flex().flex_row().items_center()
}

pub fn actions_row() -> Div {
    row().mt(px(20.)).justify_end().gap(px(8.))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ai,
    Danger,
    Accent,
}

pub fn badge(text: impl Into<SharedString>, tone: Tone, roles: Roles) -> Div {
    div()
        .flex_none()
        .px(px(4.))
        .h(px(16.))
        .flex()
        .items_center()
        .bg(match tone {
            Tone::Ai => roles.ai,
            Tone::Danger => roles.danger,
            Tone::Accent => roles.accent,
        })
        .text_color(roles.canvas)
        .type_small()
        .weight_strong()
        .child(text.into())
}

pub fn ai_flag(roles: Roles) -> Div {
    badge("AI", Tone::Ai, roles)
}

pub fn kbd(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .flex_none()
        .px(px(4.))
        .bg(roles.fill)
        .text_color(roles.ink)
        .child(text.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pip {
    Pass,
    Fail,
    Empty,
    Ai,
    You,
    File,
    Restore,
}

/// A small data mark: a run's outcome or a Version's cause. The empty marks
/// are outlines, the only borders that carry data.
pub fn pip(kind: Pip, roles: Roles) -> Div {
    let base = div().flex_none().size(px(8.));
    match kind {
        Pip::Pass => base.bg(roles.success),
        Pip::Fail => base.border_1().border_color(roles.danger),
        Pip::Empty => base.border_1().border_dashed().border_color(roles.muted),
        Pip::Ai => base.rounded_full().bg(roles.accent),
        Pip::You => base.rounded_full().bg(roles.ink),
        Pip::File => base.border_1().border_color(roles.muted),
        Pip::Restore => base.rounded_full().border_1().border_color(roles.ink),
    }
}

/// The line the History timeline's dots sit on.
pub fn axis(roles: Roles) -> Div {
    div().h(px(1.)).bg(roles.hairline)
}

pub fn field(input: impl IntoElement, focused: bool, invalid: bool, roles: Roles) -> Div {
    div()
        .w_full()
        .px(px(8.))
        .py(px(4.))
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
        .type_body()
        .text_color(roles.ink)
        .child(input)
}

pub fn problem_list(problems: Vec<String>, roles: Roles) -> Div {
    div()
        .mt(px(4.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .type_small()
        .text_color(roles.danger)
        .children(problems.into_iter().map(|problem| div().child(problem)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Neutral,
    Success,
    Danger,
}

pub fn toast_frame(id: impl Into<ElementId>, tone: ToastTone, roles: Roles) -> Stateful<Div> {
    let (background, text) = match tone {
        ToastTone::Neutral => (roles.overlay, roles.ink),
        ToastTone::Success => (roles.success_fill, roles.success),
        ToastTone::Danger => (roles.danger_fill, roles.danger),
    };
    div()
        .id(id)
        .bg(background)
        .px(px(16.))
        .py(px(8.))
        .type_body()
        .text_color(text)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.))
}

/// Reports an element's size after layout, for overlays that place
/// themselves by their real height.
pub fn measure(on_size: impl Fn(Size<Pixels>, &mut App) + 'static) -> impl IntoElement {
    let on_size = Rc::new(on_size);
    canvas(
        move |bounds, _, cx| on_size(bounds.size, cx),
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The Evals icon: four bars of a meter. It breathes while a run goes on.
pub fn gauge_icon(busy: bool, roles: Roles) -> AnyElement {
    let bar = |height: f32| div().w(px(2.)).h(px(height)).bg(roles.ink);
    let icon = div()
        .w(px(12.))
        .h(px(14.))
        .flex()
        .flex_row()
        .items_end()
        .justify_between()
        .child(bar(5.))
        .child(bar(9.))
        .child(bar(6.5))
        .child(bar(11.5));
    if busy {
        icon.with_animation(
            "evals-breathe",
            Animation::new(std::time::Duration::from_millis(3600))
                .repeat()
                .with_easing(bounce(ease_in_out)),
            |icon, delta| icon.opacity(1. - 0.75 * delta),
        )
        .into_any_element()
    } else {
        icon.into_any_element()
    }
}

pub fn list_icon(roles: Roles) -> Div {
    let line = || {
        row()
            .w(px(12.))
            .justify_between()
            .child(div().size(px(2.)).bg(roles.ink))
            .child(div().w(px(8.)).h(px(2.)).bg(roles.ink))
    };
    div()
        .w(px(14.))
        .h(px(10.))
        .flex()
        .flex_col()
        .justify_between()
        .child(line())
        .child(line())
        .child(line())
}
