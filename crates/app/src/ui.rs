//! The shared components. Every size, weight, border, hover and click
//! affordance of the UI is decided here, so the screens only arrange them.
//! Hierarchy is carried by color: two text sizes, and borders only for the
//! keyboard focus ring, an invalid field and the empty data marks.

use std::rc::Rc;

use gpui_kit::{base::TextViewStyle, prelude::FluentBuilder as _, *};

use crate::theme::Roles;

pub const TEXT_SMALL: Pixels = px(12.);
pub const TEXT_BODY: Pixels = px(14.);
/// The Document's own text, in the editor and wherever a Version is read.
pub const TEXT_DOCUMENT: Pixels = px(17.);
pub const UI_FAMILY: &str = "Inter";
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

    fn type_ui(self) -> Self {
        self.font_family(UI_FAMILY)
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
        })
        .text_color(roles.canvas)
        .type_small()
        .weight_strong()
        .child(text.into())
}

/// A small label on a fill, such as an Eval flag.
pub fn tag(text: impl Into<SharedString>, danger: bool, roles: Roles) -> Div {
    div()
        .flex_none()
        .px(px(8.))
        .type_small()
        .bg(if danger {
            roles.danger_fill
        } else {
            roles.fill
        })
        .text_color(if danger { roles.danger } else { roles.ink })
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
        Pip::Ai => base.rounded_full().bg(roles.ai),
        Pip::You => base.rounded_full().bg(roles.ink),
        Pip::File => base.border_1().border_color(roles.muted),
        Pip::Restore => base.rounded_full().border_1().border_color(roles.ink),
    }
}

/// A column of a chart that shows its tooltip on hover.
pub fn chart_slot(id: impl Into<ElementId>, roles: Roles) -> Stateful<Div> {
    div().id(id).hover(move |this| this.bg(roles.fill))
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

/// The closed state of a picker: a field that unfolds its options on click.
pub fn select_field(
    id: impl Into<ElementId>,
    open: bool,
    disabled: bool,
    roles: Roles,
) -> Stateful<Div> {
    let base = div()
        .id(id)
        .w_full()
        .h(px(32.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .bg(if open { roles.fill_hover } else { roles.fill })
        .border_1()
        .border_color(if open { roles.accent } else { roles.fill })
        .type_body();
    if disabled {
        return base.text_color(roles.faint);
    }
    base.text_color(roles.ink)
        .cursor_pointer()
        .hover(|this| this.bg(roles.fill_hover))
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

const HEADING_SCALE: [f32; 6] = [1.85, 1.45, 1.2, 1.05, 1., 1.];

fn heading_size(level: u8) -> Pixels {
    TEXT_DOCUMENT * HEADING_SCALE[usize::from(level.clamp(1, 6)) - 1]
}

pub fn markdown_style(roles: Roles) -> TextViewStyle {
    let heading_color = roles.heading;
    TextViewStyle::default()
        .with_dark(roles.dark)
        .with_foreground(roles.ink)
        .with_muted_foreground(roles.muted)
        .with_link(roles.accent)
        .with_selection(roles.selection)
        .with_code_background(roles.raised)
        .with_border(roles.hairline)
        .with_paragraph_gap(rems(1.))
        .with_inline_code(HighlightStyle {
            color: Some(roles.ink),
            background_color: Some(roles.raised),
            ..Default::default()
        })
        .with_heading(move |level| {
            let mut style = StyleRefinement::default();
            style.text.font_size = Some(heading_size(level).into());
            style.text.font_weight = Some(FontWeight::BOLD);
            style.text.color = Some(heading_color);
            style
        })
}

pub const MOTION_ENTER: std::time::Duration = std::time::Duration::from_millis(260);
pub const MOTION_TOAST: std::time::Duration = std::time::Duration::from_millis(320);
pub const MOTION_HOVER_DELAY: std::time::Duration = std::time::Duration::from_millis(160);
pub const MOTION_LEAVE: std::time::Duration = std::time::Duration::from_millis(180);
pub const MOTION_GROW: std::time::Duration = std::time::Duration::from_millis(420);
const CASCADE_STEP_MS: u64 = 30;
const CASCADE_MAX_STEPS: usize = 8;
const GROW_STEP_MS: u64 = 24;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Enter {
    Rise(f32),
    Slide(f32),
    Lift(f32),
}

const MOTION_LOOP_FPS: f32 = 30.;

pub fn ease_in_out_sine(delta: f32) -> f32 {
    0.5 - 0.5 * (std::f32::consts::PI * delta).cos()
}

pub fn ease_out_expo(delta: f32) -> f32 {
    if delta >= 1. {
        1.
    } else {
        1. - 2f32.powf(-10. * delta)
    }
}

pub fn enter<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    from: Enter,
    duration: std::time::Duration,
) -> AnimationElement<E> {
    entering(id, element, from, std::time::Duration::ZERO, duration)
}

pub fn enter_after<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    from: Enter,
    delay: std::time::Duration,
) -> AnimationElement<E> {
    entering(id, element, from, delay, MOTION_ENTER)
}

fn entering<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    from: Enter,
    delay: std::time::Duration,
    duration: std::time::Duration,
) -> AnimationElement<E> {
    let lead = delay.as_secs_f32() / (delay + duration).as_secs_f32();
    element.with_animation(
        id,
        Animation::new(delay + duration).with_easing(move |t| delayed(ease_out_expo, lead, t)),
        move |element, delta| {
            let rest = 1. - delta;
            let (x, y) = enter_offset(from, rest);
            let element = element.opacity(delta);
            match from {
                Enter::Lift(_) => element.relative().top(px(y)),
                Enter::Rise(_) => element.mt(px(y)),
                Enter::Slide(_) => element.ml(px(x)),
            }
        },
    )
}

pub fn enter_in_turn<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    from: Enter,
    index: usize,
) -> AnimationElement<E> {
    entering(
        id,
        element,
        from,
        cascade_delay(index, CASCADE_STEP_MS, CASCADE_MAX_STEPS),
        MOTION_ENTER,
    )
}

pub fn cascade_delay(index: usize, step_ms: u64, max_steps: usize) -> std::time::Duration {
    std::time::Duration::from_millis(step_ms * index.min(max_steps) as u64)
}

pub fn ease_out_quart(delta: f32) -> f32 {
    1. - (1. - delta.clamp(0., 1.)).powi(4)
}

pub fn leave<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    to: Enter,
) -> AnimationElement<E> {
    element.with_animation(
        id,
        Animation::new(MOTION_LEAVE).with_easing(ease_out_quart),
        move |element, delta| {
            let (x, y) = enter_offset(to, delta);
            let element = element.opacity(1. - delta);
            match to {
                Enter::Lift(_) => element.relative().top(px(y)),
                Enter::Rise(_) => element.mt(px(y)),
                Enter::Slide(_) => element.ml(px(x)),
            }
        },
    )
}

pub fn grow_height(
    id: impl Into<ElementId>,
    bar: Div,
    full: Pixels,
    index: usize,
) -> AnimationElement<Div> {
    growing(id, bar, index, move |bar, delta| bar.h(full * delta))
}

pub fn grow_width(
    id: impl Into<ElementId>,
    bar: Div,
    full: Pixels,
    index: usize,
) -> AnimationElement<Div> {
    growing(id, bar, index, move |bar, delta| bar.w(full * delta))
}

fn growing(
    id: impl Into<ElementId>,
    bar: Div,
    index: usize,
    size: impl Fn(Div, f32) -> Div + 'static,
) -> AnimationElement<Div> {
    let delay = cascade_delay(index, GROW_STEP_MS, 14);
    let lead = delay.as_secs_f32() / (delay + MOTION_GROW).as_secs_f32();
    bar.with_animation(
        id,
        Animation::new(delay + MOTION_GROW).with_easing(move |t| delayed(ease_out_expo, lead, t)),
        size,
    )
}

pub fn delayed(easing: impl Fn(f32) -> f32, lead: f32, t: f32) -> f32 {
    if t <= lead {
        0.
    } else {
        easing((t - lead) / (1. - lead))
    }
}

pub fn enter_offset(from: Enter, rest: f32) -> (f32, f32) {
    match from {
        Enter::Rise(offset) | Enter::Lift(offset) => (0., offset * rest),
        Enter::Slide(offset) => (offset * rest, 0.),
    }
}

pub fn breathe<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
) -> AnimationElement<E> {
    element.with_animation(
        id,
        Animation::new(std::time::Duration::from_millis(1600))
            .repeat()
            .with_max_fps(MOTION_LOOP_FPS)
            .with_easing(bounce(ease_in_out_sine)),
        |element, delta| element.opacity(1. - 0.55 * delta),
    )
}

pub fn ai_dots(roles: Roles, still: bool) -> AnyElement {
    const PERIOD_MS: f32 = 1200.;
    const STAGGER_MS: f32 = 160.;
    row()
        .gap(px(4.))
        .children((0usize..3).map(move |index| {
            let shift = STAGGER_MS * index as f32 / PERIOD_MS;
            let dot = div().size(px(3.)).bg(roles.canvas);
            if still {
                return dot.into_any_element();
            }
            dot.with_animation(
                ("ai-dot", index),
                Animation::new(std::time::Duration::from_millis(PERIOD_MS as u64))
                    .repeat()
                    .with_max_fps(MOTION_LOOP_FPS),
                move |dot, delta| dot.opacity(dot_opacity((delta - shift).rem_euclid(1.))),
            )
            .into_any_element()
        }))
        .into_any_element()
}

pub fn dot_opacity(phase: f32) -> f32 {
    let level = if phase < 0.4 {
        ease_in_out_sine(phase / 0.4)
    } else {
        ease_in_out_sine((1. - phase) / 0.6)
    };
    0.35 + 0.65 * level
}

#[cfg(test)]
mod tests {
    use super::{
        Enter, cascade_delay, delayed, dot_opacity, ease_in_out_sine, ease_out_expo,
        ease_out_quart, enter_offset,
    };

    #[test]
    fn a_rise_starts_below_a_drop_above_a_slide_on_its_side() {
        assert_eq!(enter_offset(Enter::Rise(6.), 1.), (0., 6.));
        assert_eq!(enter_offset(Enter::Rise(-4.), 1.), (0., -4.));
        assert_eq!(enter_offset(Enter::Slide(-16.), 0.5), (-8., 0.));
        assert_eq!(enter_offset(Enter::Lift(10.), 0.), (0., 0.));
    }

    #[test]
    fn a_cascade_steps_each_row_and_stops_growing() {
        let step = |index| cascade_delay(index, 30, 8).as_millis();
        assert_eq!(step(0), 0);
        assert_eq!(step(3), 90);
        assert_eq!(step(50), 240);
    }

    #[test]
    fn ease_out_quart_lands_on_one_and_clamps() {
        assert_eq!(ease_out_quart(0.), 0.);
        assert_eq!(ease_out_quart(1.), 1.);
        assert!(ease_out_quart(0.5) > 0.9);
        assert_eq!(ease_out_quart(2.), 1.);
    }

    #[test]
    fn a_delay_holds_the_start_then_runs_the_easing() {
        let linear = |t: f32| t;
        assert_eq!(delayed(linear, 0.25, 0.2), 0.);
        assert!((delayed(linear, 0.25, 0.625) - 0.5).abs() < 1e-6);
        assert_eq!(delayed(linear, 0., 0.3), 0.3);
    }

    #[test]
    fn ease_in_out_sine_is_symmetric() {
        assert!(ease_in_out_sine(0.).abs() < 1e-6);
        assert!((ease_in_out_sine(0.5) - 0.5).abs() < 1e-6);
        assert!((ease_in_out_sine(1.) - 1.).abs() < 1e-6);
    }

    #[test]
    fn ease_out_expo_starts_fast_and_lands_on_one() {
        assert_eq!(ease_out_expo(0.), 0.);
        assert!(ease_out_expo(0.2) > 0.7);
        assert_eq!(ease_out_expo(1.), 1.);
    }

    #[test]
    fn a_dot_is_faint_at_the_ends_and_full_at_forty_percent() {
        assert!((dot_opacity(0.) - 0.35).abs() < 1e-5);
        assert!((dot_opacity(0.4) - 1.).abs() < 1e-5);
        assert!((dot_opacity(1.) - 0.35).abs() < 1e-5);
        assert!(dot_opacity(0.7) > 0.35 && dot_opacity(0.7) < 1.);
    }
}
