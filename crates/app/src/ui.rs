//! The shared components. Every size, weight, border, hover and click
//! affordance of the UI is decided here, so the screens only arrange them.
//! Hierarchy is carried by color: two text sizes, and borders only for the
//! keyboard focus ring, an invalid field and the empty data marks.

use std::rc::Rc;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::{base::TextViewStyle, prelude::FluentBuilder as _, *};

use desafinado_features::caret::{CaretSettings, CaretShape};

use crate::theme::Roles;

pub const TEXT_SMALL: Pixels = px(12.);
pub const TEXT_BODY: Pixels = px(14.);
/// The Document's own text, in the editor and wherever a Version is read.
pub const TEXT_DOCUMENT: Pixels = px(17.);
pub const UI_FAMILY: &str = "Inter";
pub const MONO_FAMILY: &str = "Noto Sans Mono";
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
        .when(size == ButtonSize::Regular, Type::type_body)
        .when(size == ButtonSize::Small, Type::type_small)
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
    base.when_some(background, gpui_kit::Styled::bg)
        .when(kind == ButtonKind::Strong, Type::weight_strong)
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

pub fn ask_option_row(id: impl Into<ElementId>, active: bool, roles: Roles) -> Stateful<Div> {
    ai_option_row(id, active, roles)
        .min_h(px(28.))
        .px(px(8.))
        .py(px(4.))
        .gap(px(8.))
        .justify_between()
}

/// The label of an Ask AI row: wraps inside the bubble, up to three lines, then an ellipsis.
pub fn ask_option_label(text: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .line_clamp(3)
        .text_ellipsis()
        .child(text)
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
        .hover(gpui_kit::Styled::underline)
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

/// A cell of the Timeline's strip, which stands for a run of Versions: a bar in the accent
/// color where the dots show, in the AI color where accepted Suggestions are a good share of
/// the run, and the scrollbar thumb's color otherwise: plain runs are the track the data sits on,
/// like a scrollbar's, so the colored runs stand out. The whole slot takes the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellState {
    Plain,
    Ai,
    Shown,
}

pub fn strip_cell(id: impl Into<ElementId>, state: CellState, roles: Roles) -> Stateful<Div> {
    let color = match state {
        CellState::Plain => roles.scrollbar,
        CellState::Ai => roles.ai,
        CellState::Shown => roles.accent,
    };
    div()
        .id(id)
        .relative()
        .flex_1()
        .h(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |this| this.bg(roles.fill))
        .child(div().w(px(4.)).h(px(12.)).bg(color))
}

/// A Note's mark in the margin: its number on the AI tint, the AI color once open, and plain
/// fill when the text under it changed since the AI read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkState {
    Plain,
    Open,
    Stale,
}

pub fn note_mark(
    id: impl Into<ElementId>,
    number: usize,
    state: MarkState,
    roles: Roles,
) -> Stateful<Div> {
    let (background, text) = match state {
        MarkState::Open => (roles.ai, roles.canvas),
        MarkState::Stale => (roles.fill, roles.muted),
        MarkState::Plain => (roles.ai_fill, roles.ai),
    };
    div()
        .id(id)
        .size(px(24.))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .type_small()
        .weight_strong()
        .bg(background)
        .text_color(text)
        .cursor_pointer()
        .hover(move |this| this.bg(roles.ai).text_color(roles.canvas))
        .child(number.to_string())
}

/// A chip at the top or the foot of the text that counts the Suggestions past that edge of the
/// screen and goes to the nearest one: the AI tint, the AI color under the pointer.
pub fn edge_chip(id: impl Into<ElementId>, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(12.))
        .py(px(4.))
        .rounded_full()
        .type_small()
        .weight_medium()
        .bg(roles.ai_fill)
        .text_color(roles.ai)
        .cursor_pointer()
        .hover(move |this| this.bg(roles.ai).text_color(roles.canvas))
}

/// The requests tray: a raised list in the editor's corner, set apart by its fill.
pub fn jobs_tray(id: impl Into<ElementId>, width: Pixels, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .w(width)
        .py(px(4.))
        .flex()
        .flex_col()
        .bg(roles.overlay)
        .text_color(roles.ink)
        .type_body()
}

/// One request in the tray. A row that goes somewhere answers the pointer.
pub fn job_row(id: impl Into<ElementId>, goes: bool, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(12.))
        .py(px(8.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .when(goes, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(roles.fill_hover))
        })
}

/// A request's state: a breathing AI mark while the model works, a success mark once it answers.
pub fn job_mark(id: impl Into<ElementId>, working: bool, roles: Roles) -> AnyElement {
    if working {
        breathe(id, div().flex_none().size(px(8.)).bg(roles.ai)).into_any_element()
    } else {
        pip(Pip::Pass, roles).into_any_element()
    }
}

/// What a request asked for, in the AI color, over where it is and how long it has run.
pub fn job_text(
    label: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    roles: Roles,
) -> Div {
    let line = |text: SharedString| {
        div()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(text)
    };
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(line(label.into()).text_color(roles.ai))
        .child(line(detail.into()).type_small().text_color(roles.muted))
}

/// The key that does what a chip does, quieter than the chip's own words.
pub fn chip_key(text: impl Into<SharedString>) -> Div {
    div().weight_regular().child(text.into())
}

/// The text map beside the column: the whole Document in miniature, one bar per row of text.
/// Plain rows take the scrollbar's color, headings the muted one, Suggestions the AI color, a
/// failed one danger and the one under the caret the heading color; the rows on screen sit on
/// the fill. Clicking anywhere goes there.
pub fn text_map_frame(id: impl Into<ElementId>, width: Pixels, height: Pixels) -> Stateful<Div> {
    div().id(id).relative().w(width).h(height).cursor_pointer()
}

pub fn text_map_band(top: Pixels, height: Pixels, roles: Roles) -> Div {
    div()
        .absolute()
        .left(px(-4.))
        .right(px(-4.))
        .top(top)
        .h(height)
        .bg(roles.fill)
}

pub fn text_map_bar(
    tone: desafinado_core::text_map::MarkTone,
    top: Pixels,
    height: Pixels,
    width: Pixels,
    roles: Roles,
) -> Div {
    use desafinado_core::text_map::MarkTone;
    let (color, height) = match tone {
        MarkTone::Text => (roles.scrollbar, height),
        MarkTone::Heading => (roles.muted, height),
        MarkTone::Suggestion => (roles.ai, height.max(px(2.))),
        MarkTone::Failed => (roles.danger, height.max(px(2.))),
        MarkTone::Active => (roles.heading, height.max(px(2.))),
    };
    div()
        .absolute()
        .left_0()
        .top(top)
        .w(width)
        .h(height)
        .bg(color)
}

/// The column beside the text that holds a Comment review: General notes and the list of Notes.
pub fn side_panel(id: impl Into<ElementId>, width: Pixels, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .w(width)
        .flex_none()
        .h_full()
        .p(px(16.))
        .flex()
        .flex_col()
        .gap(px(16.))
        .bg(roles.raised)
        .text_color(roles.ink)
        .type_body()
}

/// A short label in the AI color, such as a Note's kind.
pub fn ai_label(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .flex_none()
        .type_small()
        .weight_strong()
        .text_color(roles.ai)
        .child(text.into())
}

/// A Note in the panel's list: number, title and kind on one row.
pub fn note_row(id: impl Into<ElementId>, open: bool, roles: Roles) -> Stateful<Div> {
    option_row(id, open, false, roles)
        .px(px(8.))
        .py(px(4.))
        .gap(px(8.))
        .items_start()
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

/// A block-level Replacement, drawn in the rows the editor reserves below the Original
/// (ADR 0003): the Document's type on the AI tint, in the AI color.
pub fn replacement_block(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    width: Pixels,
    line_height: Pixels,
    roles: Roles,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .w(width)
        .px(px(12.))
        .py(px(4.))
        .bg(roles.ai_fill)
        .text_color(roles.ai)
        .type_ui()
        .type_document()
        .line_height(line_height)
        .child(text.into())
}

pub fn popover_frame(id: impl Into<ElementId>, roles: Roles) -> Stateful<Div> {
    popover_surface(roles).id(id).occlude()
}

pub fn input_cover(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).absolute().inset_0().occlude()
}

pub fn scrolled(content: impl IntoElement, handle: &ScrollHandle) -> Div {
    div()
        .relative()
        .size_full()
        .flex()
        .flex_col()
        .child(content)
        .vertical_scrollbar(handle)
}

pub fn scrolled_rest(content: impl IntoElement, handle: &ScrollHandle) -> Div {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .flex()
        .flex_col()
        .child(content)
        .vertical_scrollbar(handle)
}

pub fn popover_surface(roles: Roles) -> Div {
    div()
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

/// The widest the AI caret's flag grows; a longer instruction ends in an ellipsis.
pub const AI_CARET_FLAG_MAX_WIDTH: Pixels = px(240.);

/// The flag over the AI caret: "AI", the instruction on one line, and the working dots.
pub fn ai_caret_flag(label: impl Into<SharedString>, roles: Roles, still: bool) -> Div {
    ai_flag(roles)
        .max_w(AI_CARET_FLAG_MAX_WIDTH)
        .gap(px(4.))
        .whitespace_nowrap()
        .child(
            div()
                .min_w_0()
                .truncate()
                .weight_regular()
                .child(label.into()),
        )
        .child(div().flex_none().child(ai_dots(roles, still)))
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
        |_, (), _, _| {},
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
        .with_inline_code_style(code_type())
        .with_code_block(code_block_style())
        .with_heading(move |level| {
            let mut style = StyleRefinement::default();
            style.text.font_size = Some(heading_size(level).into());
            style.text.font_weight = Some(FontWeight::BOLD);
            style.text.color = Some(heading_color);
            style
        })
}

fn code_type() -> StyleRefinement {
    let mut style = StyleRefinement::default();
    style.text.font_family = Some(MONO_FAMILY.into());
    style
}

fn code_block_style() -> StyleRefinement {
    let mut style = code_type();
    style.text.font_size = Some(TEXT_BODY.into());
    style.text.white_space = Some(WhiteSpace::Nowrap);
    style.display = Some(Display::Flex);
    style.overflow.x = Some(Overflow::Scroll);
    style.restrict_scroll_to_axis = Some(true);
    style
}

pub fn code_block_scroll_anchor() -> impl IntoElement {
    Empty
}

pub const MOTION_ENTER: std::time::Duration = std::time::Duration::from_millis(260);
pub const MOTION_TOAST: std::time::Duration = std::time::Duration::from_millis(320);
pub const MOTION_HOVER_DELAY: std::time::Duration = std::time::Duration::from_millis(160);
pub const MOTION_LEAVE: std::time::Duration = std::time::Duration::from_millis(180);
pub const MOTION_GROW: std::time::Duration = std::time::Duration::from_millis(420);
const CASCADE_STEP_MS: u64 = 30;
const CASCADE_MAX_STEPS: usize = 8;
const GROW_STEP_MS: u64 = 24;
const GROW_MAX_STEPS: usize = 14;

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
    let lead = lead_of(delay, duration);
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

pub fn column_motion(
    id: impl Into<ElementId>,
    column: Div,
    full: Pixels,
    opening: bool,
) -> AnimationElement<Div> {
    let animation = if opening {
        Animation::new(MOTION_ENTER).with_easing(ease_out_expo)
    } else {
        Animation::new(MOTION_LEAVE).with_easing(ease_out_quart)
    };
    column.with_animation(id, animation, move |column, delta| {
        column.w(full * column_share(opening, delta))
    })
}

pub fn column_share(opening: bool, delta: f32) -> f32 {
    if opening { delta } else { 1. - delta }
}

pub fn glide<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    from: f32,
) -> AnimationElement<E> {
    element.with_animation(
        id,
        Animation::new(MOTION_ENTER).with_easing(ease_out_expo),
        move |element, delta| element.relative().top(px(from * (1. - delta))),
    )
}

pub fn collapse(id: impl Into<ElementId>, row: Div, full: Pixels) -> AnimationElement<Div> {
    row.with_animation(
        id,
        Animation::new(MOTION_LEAVE).with_easing(ease_out_quart),
        move |row, delta| row.h(full * (1. - delta)).opacity(1. - delta),
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
    let delay = cascade_delay(index, GROW_STEP_MS, GROW_MAX_STEPS);
    let lead = lead_of(delay, MOTION_GROW);
    bar.with_animation(
        id,
        Animation::new(delay + MOTION_GROW).with_easing(move |t| delayed(ease_out_expo, lead, t)),
        size,
    )
}

pub fn lead_of(delay: std::time::Duration, duration: std::time::Duration) -> f32 {
    delay.as_secs_f32() / (delay + duration).as_secs_f32()
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
    dots(roles.canvas, still)
}

/// A small button's place while the model works on it: the label and the
/// moving dots in the AI color, so the card does not look frozen.
pub fn working_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    roles: Roles,
    still: bool,
) -> Stateful<Div> {
    sized_button(
        id,
        label,
        ButtonKind::Default,
        ButtonSize::Small,
        Control::disabled(true),
        roles,
    )
    .bg(roles.fill)
    .text_color(roles.ai)
    .child(dots(roles.ai, still))
}

fn dots(color: Hsla, still: bool) -> AnyElement {
    const PERIOD_MS: f32 = 1200.;
    const STAGGER_MS: f32 = 160.;
    row()
        .gap(px(4.))
        .children((0usize..3).map(move |index| {
            let shift = STAGGER_MS * index as f32 / PERIOD_MS;
            let dot = div().size(px(3.)).bg(color);
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

/// What a caret quad is painted with: the accent, or the canvas that hides
/// the editor's own one-pixel caret while ours blinks off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaretInk {
    Accent,
    Erase,
}

/// Where the caret sits this frame, in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretSpot {
    pub origin: Point<Pixels>,
    pub height: Pixels,
    /// The width of the character after the caret, on the same row.
    pub next_width: Option<Pixels>,
    pub selecting: bool,
    /// The caret sits on a tinted fill (a Suggestion), where a canvas quad
    /// would cut a slit; the editor's own caret is left to show there.
    pub on_fill: bool,
}

const CARET_SELECTING_BLOCK: Pixels = px(2.);

/// The quads that draw the writer's caret over the editor. The editor still
/// paints its own one-pixel caret in the accent; every shape covers it or erases it.
pub fn caret_quads(
    settings: CaretSettings,
    shown: bool,
    spot: CaretSpot,
) -> Vec<(Bounds<Pixels>, CaretInk)> {
    let CaretSpot {
        origin,
        height,
        next_width,
        selecting,
        on_fill,
    } = spot;
    let line = px(f32::from(settings.width));
    let bar = |width: Pixels| (Bounds::new(origin, size(width, height)), CaretInk::Accent);
    let erase = (!on_fill).then(|| (Bounds::new(origin, size(px(1.), height)), CaretInk::Erase));
    if selecting {
        let width = if settings.shape.has_width() {
            line
        } else {
            CARET_SELECTING_BLOCK
        };
        return vec![bar(width)];
    }
    if !shown {
        return erase.into_iter().collect();
    }
    let cell = next_width.unwrap_or(height * 0.5);
    match settings.shape {
        CaretShape::Bar => vec![bar(line)],
        CaretShape::Block if next_width.is_none() => vec![bar(cell)],
        CaretShape::Block => vec![],
        CaretShape::Underline => erase
            .into_iter()
            .chain([(
                Bounds::new(point(origin.x, origin.y + height - line), size(cell, line)),
                CaretInk::Accent,
            )])
            .collect(),
    }
}

/// Paints the writer's caret over the editor, after the editor has laid out
/// this frame, so it never trails the text.
pub fn caret_layer(
    editor: Entity<gpui_kit::base::input::EditorState>,
    settings: CaretSettings,
    (shown, on_fill): (bool, bool),
    roles: Roles,
) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |_, (), window, cx| {
            let state = editor.read(cx);
            if !state.focus_handle(cx).is_focused(window) || !window.is_window_active() {
                return;
            }
            let Some(caret) = painted_caret_line(state) else {
                return;
            };
            let next_width = caret_cell(state)
                .and_then(|range| state.range_to_bounds(&range))
                .filter(|bounds| (bounds.top() - caret.top()).abs() < px(1.))
                .map(|bounds| bounds.size.width)
                .filter(|width| *width > px(0.));
            let scale = window.scale_factor();
            let spot = CaretSpot {
                origin: point((caret.origin.x * scale).round() / scale, caret.origin.y),
                height: caret.size.height,
                next_width,
                selecting: !state.selected_range().is_empty(),
                on_fill,
            };
            let mask = ContentMask {
                bounds: state.input_bounds(),
            };
            window.with_content_mask(Some(mask), |window| {
                for (bounds, ink) in caret_quads(settings, shown, spot) {
                    let color = match ink {
                        CaretInk::Accent => roles.accent,
                        CaretInk::Erase => roles.canvas,
                    };
                    window.paint_quad(fill(bounds, color));
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

/// The line the editor's own caret sits on this frame, as tall as the line.
/// It follows the editor's caret to the end of a wrapped row, where the
/// offset alone would place it at the start of the next one.
fn painted_caret_line(state: &gpui_kit::base::input::EditorState) -> Option<Bounds<Pixels>> {
    let geometry = state.prepaint_caret_geometry()?;
    let painted = geometry.cursor_bounds();
    let line = geometry.line_height();
    let top = painted.top() + geometry.scroll_offset().y - (line - painted.size.height) / 2.;
    Some(Bounds::new(point(painted.left(), top), size(px(1.), line)))
}

/// The block caret: the accent behind the next character, and the character
/// in the canvas color, as Vim draws it.
pub fn caret_block_ink(roles: Roles) -> (Hsla, HighlightStyle) {
    (
        roles.accent,
        HighlightStyle {
            color: Some(roles.canvas),
            ..HighlightStyle::default()
        },
    )
}

/// The cell a block or underline caret covers: the character after the cursor, unless
/// inserted text shows at the cursor, where the caret sits before it.
pub fn caret_cell(state: &gpui_kit::base::input::EditorState) -> Option<std::ops::Range<usize>> {
    let cursor = state.cursor();
    caret_cell_in(state.text(), cursor, state.inlays())
}

fn caret_cell_in(
    text: &gpui_kit::base::input::Rope,
    cursor: usize,
    inlays: &[gpui_kit::base::input::Inlay],
) -> Option<std::ops::Range<usize>> {
    if inlays.iter().any(|inlay| inlay.offset() == cursor) {
        return None;
    }
    next_character(text, cursor)
}

/// The byte range of the character after `cursor`, unless it ends a line.
fn next_character(
    text: &gpui_kit::base::input::Rope,
    cursor: usize,
) -> Option<std::ops::Range<usize>> {
    use gpui_kit::base::input::RopeExt as _;
    let ch = text.char_at(cursor)?;
    (ch != '\n').then(|| cursor..cursor + ch.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::{CaretInk, CaretSpot, caret_quads};
    use super::{
        Enter, cascade_delay, column_share, delayed, dot_opacity, ease_in_out_sine, ease_out_expo,
        ease_out_quart, enter_offset, lead_of,
    };
    use desafinado_features::caret::{CaretSettings, CaretShape};
    use gpui_kit::{point, px};

    fn spot(next: Option<f32>, selecting: bool) -> CaretSpot {
        CaretSpot {
            origin: point(px(100.), px(40.)),
            height: px(24.),
            next_width: next.map(px),
            selecting,
            on_fill: false,
        }
    }

    fn shape(shape: CaretShape, width: u8) -> CaretSettings {
        CaretSettings {
            shape,
            ..CaretSettings::default()
        }
        .with_width(width)
    }

    #[test]
    fn a_bar_is_as_wide_as_the_setting() {
        let quads = caret_quads(shape(CaretShape::Bar, 4), true, spot(Some(9.), false));
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].0.size.width, px(4.));
        assert_eq!(quads[0].0.size.height, px(24.));
        assert_eq!(quads[0].1, CaretInk::Accent);
    }

    #[test]
    fn a_hidden_caret_only_erases_the_editor_s_own() {
        for kind in CaretShape::ALL {
            let quads = caret_quads(shape(kind, 3), false, spot(Some(9.), false));
            assert_eq!(quads.len(), 1, "{kind:?}");
            assert_eq!(quads[0].1, CaretInk::Erase);
            assert_eq!(quads[0].0.size.width, px(1.));
        }
    }

    #[test]
    fn the_caret_cell_is_the_next_character_unless_inserted_text_shows_there() {
        let text = gpui_kit::base::input::Rope::from("ab\ncd");
        let inlay =
            gpui_kit::base::input::Inlay::new(1, "xyz", gpui_kit::HighlightStyle::default());
        assert_eq!(
            super::caret_cell_in(&text, 0, std::slice::from_ref(&inlay)),
            Some(0..1)
        );
        assert_eq!(super::caret_cell_in(&text, 1, &[inlay]), None);
        assert_eq!(super::caret_cell_in(&text, 2, &[]), None);
    }

    #[test]
    fn an_underline_spans_the_next_character_at_the_bottom() {
        let quads = caret_quads(shape(CaretShape::Underline, 2), true, spot(Some(9.), false));
        let (foot, ink) = quads[1];
        assert_eq!(quads[0].1, CaretInk::Erase);
        assert_eq!(ink, CaretInk::Accent);
        assert_eq!(foot.size.width, px(9.));
        assert_eq!(foot.size.height, px(2.));
        assert_eq!(foot.bottom(), px(64.));
    }

    #[test]
    fn a_block_is_a_decoration_except_at_the_end_of_a_line() {
        assert!(caret_quads(shape(CaretShape::Block, 3), true, spot(Some(9.), false)).is_empty());
        let end = caret_quads(shape(CaretShape::Block, 3), true, spot(None, false));
        assert_eq!(end[0].0.size.width, px(12.));
    }

    #[test]
    fn on_a_suggestion_fill_nothing_is_erased() {
        let on_fill = CaretSpot {
            on_fill: true,
            ..spot(Some(9.), false)
        };
        assert!(caret_quads(shape(CaretShape::Bar, 3), false, on_fill).is_empty());
        let underline = caret_quads(shape(CaretShape::Underline, 3), true, on_fill);
        assert_eq!(underline.len(), 1);
        assert_eq!(underline[0].1, CaretInk::Accent);
    }

    #[test]
    fn a_selection_shows_a_steady_bar() {
        let bar = caret_quads(shape(CaretShape::Underline, 5), false, spot(Some(9.), true));
        assert_eq!(bar.len(), 1);
        assert_eq!(bar[0].0.size.width, px(5.));
        let block = caret_quads(shape(CaretShape::Block, 5), false, spot(Some(9.), true));
        assert_eq!(block[0].0.size.width, px(2.));
    }

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
    fn the_lead_is_the_share_of_time_spent_waiting() {
        let ms = std::time::Duration::from_millis;
        assert_eq!(lead_of(ms(0), ms(260)), 0.);
        assert!((lead_of(ms(160), ms(240)) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn a_column_opens_from_nothing_and_closes_to_nothing() {
        assert_eq!(column_share(true, 0.), 0.);
        assert_eq!(column_share(true, 1.), 1.);
        assert_eq!(column_share(false, 0.), 1.);
        assert_eq!(column_share(false, 1.), 0.);
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
