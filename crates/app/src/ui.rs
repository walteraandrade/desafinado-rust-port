//! The shared components. Every size, weight, border, hover and click
//! affordance of the UI is decided here, so the screens only arrange them.
//! Hierarchy is carried by color: two text sizes, and borders only for the
//! keyboard focus ring, an invalid field and the empty data marks.
//! A new way out (something that leaves with motion) uses `Presence`, from ginga.

use std::rc::Rc;

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarHandle};
use gpui_kit::{base::TextViewStyle, prelude::FluentBuilder as _, *};

use desafinado_ai::list_rows::ListMotion;
use desafinado_features::caret::{CaretSettings, CaretShape};
use ginga_gpui::LookExt as _;

use crate::theme::Roles;

pub const TEXT_SMALL: Pixels = px(12.);
pub const TEXT_BODY: Pixels = px(14.);
/// The Document's own text, in the editor and wherever a Version is read.
pub const TEXT_DOCUMENT: Pixels = px(17.);
pub const UI_FAMILY: &str = "Inter";
pub const MONO_FAMILY: &str = "Noto Sans Mono";
const WORDMARK_FAMILY: &str = "League Spartan";

pub fn body_line_height(window: &Window) -> Pixels {
    let mut style = window.text_style();
    style.font_size = TEXT_BODY.into();
    style.line_height_in_pixels(window.rem_size())
}

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

/// A small button that keeps its look but takes no pointer: no click, hover
/// or hand cursor, as on a surface on its way out.
pub fn small_button_at_rest(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    roles: Roles,
) -> Stateful<Div> {
    any_button(
        id,
        label,
        kind,
        ButtonSize::Small,
        Control::default(),
        false,
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
    any_button(id, label, kind, size, control, true, roles)
}

fn any_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    size: ButtonSize,
    control: Control,
    takes_pointer: bool,
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
        .child(label.into());
    let on_fill = matches!(kind, ButtonKind::Strong | ButtonKind::Primary) && !control.disabled;
    let base = base.when(control.focused, |this| {
        this.border_color(if on_fill { roles.canvas } else { roles.accent })
    });
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
    let base = base
        .when_some(background, gpui_kit::Styled::bg)
        .when(kind == ButtonKind::Strong, Type::weight_strong)
        .text_color(text);
    if !takes_pointer {
        return base;
    }
    base.cursor_pointer().hover(move |this| {
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

/// Places `marker`, when there is one, over the whole element: used to scroll a control into
/// view when the keyboard reaches it.
pub fn with_reveal(element: Stateful<Div>, marker: Option<impl IntoElement>) -> Stateful<Div> {
    element.relative().when_some(marker, ParentElement::child)
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

/// An Ask AI row in a bubble on its way out: drawn the same, with no pointer,
/// since it takes no clicks.
pub fn leaving_ask_option_row(
    id: impl Into<ElementId>,
    active: bool,
    roles: Roles,
) -> Stateful<Div> {
    ask_option_row(id, active, roles).cursor_default()
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
        .occlude()
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

/// A request's state: an AI mark at `opacity` of its [`breath`] while the
/// model works, a success mark once it answers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobMark {
    Working { opacity: f32 },
    Answered,
}

pub fn job_mark(mark: JobMark, roles: Roles) -> AnyElement {
    match mark {
        JobMark::Working { opacity } => div()
            .flex_none()
            .size(px(8.))
            .bg(roles.ai)
            .opacity(opacity)
            .into_any_element(),
        JobMark::Answered => pip(Pip::Pass, roles).into_any_element(),
    }
}

/// A layer the size of the editor pane, which clips what it holds, for a view
/// that draws over the cached pane: the AI caret and the tray's marks.
pub fn over_pane(area: Size<Pixels>) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .w(area.width)
        .h(area.height)
        .overflow_hidden()
}

/// The place of a working request's mark in the tray, left empty: the mark
/// itself breathes in a view of its own over the pane (`tray_marks_view`), so
/// its frames do not render the pane. `place` is told where the spot is laid
/// out on each frame that draws the pane; a frame that reuses the cached pane
/// does not call it.
pub fn job_mark_spot(place: impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static) -> Div {
    div().flex_none().size(px(8.)).child(
        canvas(
            move |bounds, window, cx| place(bounds, window, cx),
            |_, (), _, _| {},
        )
        .size_full(),
    )
}

/// What a request asked for over where it is and how long it has run; the mark beside it
/// carries the AI color, so the words stay readable on the row's hover fill.
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
        .child(line(label.into()).text_color(roles.ink))
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

pub fn scrolled_rest<H: ScrollbarHandle + Clone>(content: impl IntoElement, handle: &H) -> Div {
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

/// The centered text column of every page (`workspace::column_row`) and the Reading view that mirrors it.
pub const COLUMN_WIDTH: Pixels = px(768.);
pub const COLUMN_INSET: Pixels = px(24.);
pub const COLUMN_PAD_BOTTOM: Pixels = px(12.);
pub const READING_PAD_TOP: Pixels = px(16.);
/// The Reading view's old bottom space: 48 below the page and `COLUMN_PAD_BOTTOM` in the column.
pub const READING_PAD_BOTTOM: Pixels = px(60.);

pub fn column_text_width() -> Pixels {
    COLUMN_WIDTH - COLUMN_INSET * 2.
}

/// The widest the AI caret's flag grows; a longer instruction ends in an ellipsis.
pub const AI_CARET_FLAG_MAX_WIDTH: Pixels = px(240.);

/// The flag over the AI caret: "AI", the instruction on one line, and the
/// working dots at `dots`, their [`dot_levels`].
pub fn ai_caret_flag(label: impl Into<SharedString>, roles: Roles, dots: [f32; 3]) -> Div {
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
        .child(div().flex_none().child(ai_dots(roles, dots)))
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

/// The Evals icon: four bars of a meter, at `opacity`: while a run goes on it
/// follows a [`gauge_breath`], otherwise it is 1.
pub fn gauge_icon(opacity: f32, roles: Roles) -> Div {
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
    icon.opacity(opacity)
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
pub const MOTION_TRAVEL: std::time::Duration = std::time::Duration::from_millis(560);
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

/// The least time between two frames of a slow loop. GPUI draws the next one
/// at the display's following frame, so at 60 Hz it is about 20 a second, as
/// `with_max_fps(30.)` was.
const MOTION_LOOP_FRAME: std::time::Duration = std::time::Duration::from_millis(33);
const DOTS_ROUND: std::time::Duration = std::time::Duration::from_millis(1200);
const DOTS_STAGGER: f32 = 160. / 1200.;

/// A loop a view draws, which plays from its first stop each time it comes
/// back: [`Looping::frame`] while it is drawn, [`Looping::away`] on a frame
/// that leaves it out. Under reduced motion it rests on its first stop.
#[derive(Clone, Copy, Debug)]
pub struct Looping<const N: usize> {
    keys: ginga_gpui::Keyframes<f32, N>,
    drawn: bool,
}

impl<const N: usize> Looping<N> {
    fn new(keys: ginga_gpui::Keyframes<f32, N>) -> Self {
        Self { keys, drawn: false }
    }

    /// The value this frame, asking for the next one.
    pub fn frame(&mut self, window: &Window, cx: &App) -> f32 {
        if !self.drawn {
            self.keys.restart(cx);
            self.drawn = true;
        }
        self.keys.frame(window, cx)
    }

    /// The loop is not drawn this frame; it starts over when it is again.
    pub fn away(&mut self) {
        self.drawn = false;
    }
}

type LoopStops = [ginga_gpui::Stop<f32>; 2];

/// A round trip played forever: `rest` to `far` in `half` ms, and back.
fn round_trip(rest: f32, far: f32, half: u64, easing: ginga_gpui::Easing) -> LoopStops {
    [
        ginga_gpui::Stop::new(std::time::Duration::ZERO, rest, ginga_gpui::Easing::Linear),
        ginga_gpui::Stop::new(std::time::Duration::from_millis(half), far, easing),
    ]
}

fn looping(stops: LoopStops, mirrored: bool) -> Looping<2> {
    let keys = ginga_gpui::Keyframes::new(stops)
        .expect("two stops in order from zero")
        .forever()
        .at_most_every(MOTION_LOOP_FRAME);
    Looping::new(if mirrored { keys.mirrored() } else { keys })
}

fn breath_stops() -> LoopStops {
    round_trip(1., 0.45, 800, ginga_gpui::Easing::InOutSine)
}

fn gauge_stops() -> LoopStops {
    round_trip(1., 0.25, 1800, ginga_gpui::Easing::Custom(ease_in_out))
}

fn dots_stops() -> LoopStops {
    [
        ginga_gpui::Stop::new(std::time::Duration::ZERO, 0., ginga_gpui::Easing::Linear),
        ginga_gpui::Stop::new(DOTS_ROUND, 1., ginga_gpui::Easing::Linear),
    ]
}

/// The opacity of a mark while the model works: down to 0.45 and back up,
/// 1.6 s a round.
pub fn breath() -> Looping<2> {
    looping(breath_stops(), true)
}

/// The opacity of the Evals icon while a run goes on: slower and deeper than
/// a [`breath`], 3.6 s a round.
pub fn gauge_breath() -> Looping<2> {
    looping(gauge_stops(), true)
}

/// Where the working dots are in their round, from 0 to 1, 1.2 s a round;
/// [`dot_levels`] turns it into each dot's opacity.
pub fn dots_round() -> Looping<2> {
    looping(dots_stops(), false)
}

pub fn enter<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    from: Enter,
    duration: std::time::Duration,
) -> AnyElement {
    entering(id, element, from, std::time::Duration::ZERO, duration)
}

/// The looks something that enters starts from and rests at: `from`'s
/// offset and no opacity, then its place, fully opaque.
fn enter_looks(from: Enter) -> (ginga_gpui::Look, ginga_gpui::Look) {
    let (x, y) = enter_offset(from, 1.);
    (
        ginga_gpui::Look::new().offset(x, y).opacity(0.),
        ginga_gpui::Look::new().offset(0., 0.).opacity(1.),
    )
}

/// `element` arriving from `from` when it is first drawn, after `delay`. A
/// look's offset takes the place of an absolutely placed element's own, so
/// such an element enters inside a layer that covers its parent and moves
/// instead.
fn entering<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    mut element: E,
    from: Enter,
    delay: std::time::Duration,
    duration: std::time::Duration,
) -> AnyElement {
    let (start, rest) = enter_looks(from);
    let way = ginga_gpui::Tween::new(duration, ginga_gpui::Easing::OutExpo).delay(delay);
    if enters_in_a_layer(element.style()) {
        return div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(element)
            .look(id, rest, way)
            .from(start)
            .into_any_element();
    }
    element.look(id, rest, way).from(start).into_any_element()
}

/// Whether an element enters inside a layer that covers its parent: a look's
/// offset would take the place of an absolutely placed element's own.
fn enters_in_a_layer(style: &StyleRefinement) -> bool {
    style.position == Some(Position::Absolute)
}

pub fn enter_in_turn<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    element: E,
    from: Enter,
    index: usize,
) -> AnyElement {
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

/// How a side panel (the Library, Preferences) arrives and how it leaves.
pub fn panel_in_out() -> ginga_gpui::InOut {
    ginga_gpui::InOut::new(
        ginga_gpui::Tween::new(MOTION_ENTER, ginga_gpui::Easing::OutExpo),
        ginga_gpui::Tween::new(MOTION_LEAVE, ginga_gpui::Easing::OutQuart),
    )
}

/// `element` as much on screen as `progress` says: it fades and sits `slide`
/// pixels to the side of its place, by its left margin, as `progress` falls.
pub fn slid<E: Styled>(element: E, slide: f32, progress: f32) -> E {
    let (opacity, x) = slide_at(slide, progress);
    element.opacity(opacity).ml(px(x))
}

/// The opacity and the offset from its place of something that slides
/// `slide` pixels into place, `progress` of the way in.
pub fn slide_at(slide: f32, progress: f32) -> (f32, f32) {
    let progress = progress.clamp(0., 1.);
    (progress, enter_offset(Enter::Slide(slide), 1. - progress).0)
}

/// A column `progress` of its `full` width open.
pub fn column_at(column: Div, full: Pixels, progress: f32) -> Div {
    column.w(full * progress.clamp(0., 1.))
}

/// How a row of a list arrives and leaves: it lifts in and shrinks away, or
/// does neither when the desktop asks for less motion.
pub fn list_in_out(cx: &App) -> ginga_gpui::InOut {
    if cx.reduce_motion() {
        return ginga_gpui::InOut::INSTANT;
    }
    ginga_gpui::InOut::new(
        ginga_gpui::Tween::new(MOTION_ENTER, ginga_gpui::Easing::OutExpo),
        ginga_gpui::Tween::new(MOTION_LEAVE, ginga_gpui::Easing::OutQuart),
    )
}

/// How a row of a list glides to its new place.
pub fn list_glide(cx: &App) -> ginga_gpui::Transition {
    let glide = ginga_gpui::Tween::new(MOTION_ENTER, ginga_gpui::Easing::OutExpo);
    ginga_gpui::unless_reduced(glide, cx)
}

/// How the rows of a list arrive, leave and glide. The rows below a deleted
/// row close up on its way out (`ListMotion::close_up`).
pub fn list_motion(cx: &App) -> ListMotion {
    ListMotion {
        in_out: list_in_out(cx),
        glide: list_glide(cx),
    }
}

/// How far above its place a row of a list starts as it lifts in.
const LIST_ROW_LIFT: f32 = -6.;

/// A row of a list as its presence draws it this frame, `offset` away from
/// where it is laid out. An arriving row lifts in; a leaving row is cut to
/// `height`, which falls as it shrinks, and fades away.
pub fn list_row(
    row: impl IntoElement,
    phase: ginga_gpui::Phase,
    progress: f32,
    offset: f32,
    height: Pixels,
) -> Div {
    let (opacity, lift) = match phase {
        ginga_gpui::Phase::Entering => lift_at(LIST_ROW_LIFT, progress),
        ginga_gpui::Phase::Exiting => (progress.clamp(0., 1.), 0.),
        ginga_gpui::Phase::Present => (1., 0.),
    };
    div()
        .relative()
        .top(px(lift + offset))
        .when(phase == ginga_gpui::Phase::Exiting, |row| {
            row.h(height).overflow_hidden()
        })
        .opacity(opacity)
        .child(row)
}

/// The clock a view keeps for motion that lives in the plain crates.
pub type MotionClock = ginga_gpui::Clock;

/// A surface that comes and goes, kept to be drawn while it leaves.
pub type Presence<T> = ginga_gpui::Presence<T>;

/// How a toast arrives and how it leaves.
pub fn toast_in_out() -> ginga_gpui::InOut {
    ginga_gpui::InOut::new(
        ginga_gpui::Tween::new(MOTION_TOAST, ginga_gpui::Easing::OutExpo),
        ginga_gpui::Tween::new(MOTION_LEAVE, ginga_gpui::Easing::OutQuart),
    )
}

/// How the Ask bubble leaves.
pub fn bubble_out() -> ginga_gpui::Tween {
    ginga_gpui::Tween::new(MOTION_LEAVE, ginga_gpui::Easing::OutQuart)
}

/// `element` as much on screen as `progress` says: it fades and sits `rise`
/// pixels from its place as `progress` falls, by its top margin, so an
/// absolutely placed element keeps its place.
pub fn risen<E: Styled>(element: E, rise: f32, progress: f32) -> E {
    element
        .opacity(progress.clamp(0., 1.))
        .mt(px(rise * (1. - progress)))
}

/// `element` as much on screen as `progress` says: it fades in and lifts
/// `lift` pixels into its place. Entering, leaving and the toast all lift
/// with it.
pub fn lifted<E: Styled>(element: E, lift: f32, progress: f32) -> E {
    let (opacity, top) = lift_at(lift, progress);
    element.opacity(opacity).relative().top(px(top))
}

/// The opacity and the offset from its place of something that lifts `lift`
/// pixels into place, `progress` of the way in.
pub fn lift_at(lift: f32, progress: f32) -> (f32, f32) {
    let progress = progress.clamp(0., 1.);
    (progress, enter_offset(Enter::Lift(lift), 1. - progress).1)
}

/// Whether a surface that comes and goes takes clicks, keys and the focus:
/// not once it is on its way out. Toasts, menus and the Review popover follow
/// it.
pub fn takes_input(phase: ginga_gpui::Phase) -> bool {
    phase != ginga_gpui::Phase::Exiting
}

/// How a menu or a popover arrives and leaves: it rises into place and fades
/// out the way the Ask bubble does.
pub fn popover_in_out() -> ginga_gpui::InOut {
    ginga_gpui::InOut::new(
        ginga_gpui::Tween::new(MOTION_ENTER, ginga_gpui::Easing::OutExpo),
        bubble_out(),
    )
}

/// A menu or a popover on screen, which may be leaving, on its view's clock.
/// It rises in and fades out with [`popover_in_out`], or moves at once when
/// the desktop asks for less motion.
pub struct Surface<T> {
    surface: desafinado_ai::surface::Surface<T>,
    clock: MotionClock,
}

impl<T> Default for Surface<T> {
    fn default() -> Self {
        Self {
            surface: desafinado_ai::surface::Surface::default(),
            clock: MotionClock::default(),
        }
    }
}

impl<T> Surface<T> {
    /// Brings the surface in line with `wanted`, the face that should be on
    /// screen this frame; see [`desafinado_ai::surface::Surface::follow`].
    pub fn follow(&mut self, wanted: Option<T>, same: impl FnOnce(&T, &T) -> bool, cx: &App) {
        let in_out = popover_in_out();
        let in_out = ginga_gpui::InOut::new(
            ginga_gpui::unless_reduced(in_out.enter, cx),
            ginga_gpui::unless_reduced(in_out.exit, cx),
        );
        let now = self.clock.now(cx);
        self.surface.follow(wanted, same, in_out, now);
    }

    /// Takes away at once a surface that is leaving.
    pub fn drop_leaving(&mut self) {
        self.surface.drop_leaving();
    }

    /// What to draw this frame, asking for the next one while it moves.
    pub fn frame(&mut self, window: &Window, cx: &App) -> Option<ginga_gpui::Shown<'_, T>> {
        let now = self.clock.now(cx);
        let shown = self.surface.frame(now);
        if shown.as_ref().is_some_and(ginga_gpui::Shown::moving) {
            window.request_animation_frame();
        }
        shown
    }
}

/// How the AI caret travels to another place in the text: no motion at all
/// when the desktop asks for less of it.
pub fn caret_trip(cx: &App) -> ginga_gpui::Transition {
    let trip = ginga_gpui::Tween::new(MOTION_TRAVEL, ginga_gpui::Easing::OutExpo);
    ginga_gpui::unless_reduced(trip, cx)
}

/// A chart bar that grows from nothing to `full` high, the `index`th of its
/// chart a little after the one before.
pub fn grow_height(id: impl Into<ElementId>, bar: Div, full: Pixels, index: usize) -> AnyElement {
    let (start, rest) = (
        ginga_gpui::Look::new().height(0.),
        ginga_gpui::Look::new().height(f32::from(full)),
    );
    growing(id, bar, start, rest, index)
}

/// A bar that grows from nothing to `full` wide; see [`grow_height`].
pub fn grow_width(id: impl Into<ElementId>, bar: Div, full: Pixels, index: usize) -> AnyElement {
    let (start, rest) = (
        ginga_gpui::Look::new().width(0.),
        ginga_gpui::Look::new().width(f32::from(full)),
    );
    growing(id, bar, start, rest, index)
}

fn growing(
    id: impl Into<ElementId>,
    bar: Div,
    start: ginga_gpui::Look,
    rest: ginga_gpui::Look,
    index: usize,
) -> AnyElement {
    bar.look(id, rest, grow_way(index))
        .from(start)
        .into_any_element()
}

fn grow_way(index: usize) -> ginga_gpui::Tween {
    ginga_gpui::Tween::new(MOTION_GROW, ginga_gpui::Easing::OutExpo).delay(cascade_delay(
        index,
        GROW_STEP_MS,
        GROW_MAX_STEPS,
    ))
}

pub fn enter_offset(from: Enter, rest: f32) -> (f32, f32) {
    match from {
        Enter::Rise(offset) | Enter::Lift(offset) => (0., offset * rest),
        Enter::Slide(offset) => (offset * rest, 0.),
    }
}

/// The AI caret's bar, filling its place, at `opacity` of its [`breath`].
pub fn ai_caret_bar(roles: Roles, opacity: f32) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(roles.ai)
        .opacity(opacity)
}

pub fn ai_dots(roles: Roles, levels: [f32; 3]) -> Div {
    dots(roles.canvas, levels)
}

/// A small button's place while the model works on it: the label and the
/// moving dots in the AI color, so the card does not look frozen.
pub fn working_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    roles: Roles,
    dots_at: [f32; 3],
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
    .child(dots(roles.ai, dots_at))
}

fn dots(color: Hsla, levels: [f32; 3]) -> Div {
    row().gap(px(4.)).children(
        levels
            .into_iter()
            .map(move |level| div().size(px(3.)).bg(color).opacity(level)),
    )
}

/// The three working dots' opacities at `round`, where a [`dots_round`] is:
/// each dot rises and falls 160 ms after the one before. Still, they are full.
pub fn dot_levels(round: f32, still: bool) -> [f32; 3] {
    if still {
        return [1.; 3];
    }
    [0., 1., 2.].map(|index| dot_opacity((round - DOTS_STAGGER * index).rem_euclid(1.)))
}

pub fn dot_opacity(phase: f32) -> f32 {
    let sine = ginga_gpui::Easing::InOutSine;
    let level = if phase < 0.4 {
        sine.at(phase / 0.4)
    } else {
        sine.at((1. - phase) / 0.6)
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

/// A dotted line under text the spell checker marks: square dots every other dot width.
pub const SPELL_DOT: Pixels = px(2.);

pub fn dotted_line(left: Pixels, right: Pixels, top: Pixels) -> Vec<Bounds<Pixels>> {
    let mut dots = Vec::new();
    let mut x = left;
    while x + SPELL_DOT <= right + px(0.5) {
        dots.push(Bounds::new(point(x, top), size(SPELL_DOT, SPELL_DOT)));
        x += SPELL_DOT * 2.;
    }
    dots
}

/// Text the spell checker marks, in the color its dots take this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct SpellMark {
    pub range: std::ops::Range<usize>,
    pub color: Hsla,
}

/// Builds the marks that reach a byte range of the Document, from what the editor shows now.
pub type MarksIn =
    Rc<dyn Fn(&gpui_kit::base::input::EditorState, std::ops::Range<usize>) -> Vec<SpellMark>>;

/// Paints the spell checker's dots under the editor's text, after the editor has laid out
/// this frame. Each word is measured on its own, so a mark that wraps keeps its dots on
/// every row it reaches. Only the marks on rows in view are built and measured, however long
/// the line is.
pub fn spelling_layer(
    editor: Entity<gpui_kit::base::input::EditorState>,
    marks_in: MarksIn,
) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |_, (), window, cx| {
            perf_span!("spelling.paint");
            let state = editor.read(cx);
            let screen = state.input_bounds();
            let mask = ContentMask { bounds: screen };
            window.with_content_mask(Some(mask), |window| {
                let view = state.visible_row_range().map(|rows| {
                    use gpui_kit::base::input::RopeExt as _;
                    let from = state.text().line_start_offset(rows.start.saturating_sub(1));
                    from..state.text().line_start_offset(rows.end + 1).max(from)
                });
                let bytes = view.map_or(0..usize::MAX, |view| {
                    bytes_on_screen(view, |offset| row_span(state, offset), screen)
                });
                for mark in marks_in(state, bytes) {
                    for (left, right, line) in mark_rows(state, &mark.range) {
                        let top = line.top() + line.size.height * 0.5 + px(SPELL_DOT_DROP);
                        for dot in dotted_line(left, right, top) {
                            window.paint_quad(fill(dot, mark.color));
                        }
                    }
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

/// The top and bottom of the row that holds `offset`, when it is laid out.
fn row_span(state: &gpui_kit::base::input::EditorState, offset: usize) -> Option<(Pixels, Pixels)> {
    let text = state.text();
    let offset = text.floor_char_boundary(offset.min(text.len()));
    state
        .range_to_bounds(&(offset..offset))
        .map(|bounds| (bounds.top(), bounds.bottom()))
}

/// The bytes of `within` whose rows can show on `screen`, found by searching the rows by
/// position instead of measuring every mark: a mark outside them has all its dots off screen.
/// `row_of` gives the top and bottom of an offset's row, or `None` where it is not laid out,
/// which the search treats as in view.
fn bytes_on_screen(
    within: std::ops::Range<usize>,
    row_of: impl Fn(usize) -> Option<(Pixels, Pixels)>,
    screen: Bounds<Pixels>,
) -> std::ops::Range<usize> {
    let first_in_view = first_where(&within, |offset| {
        row_of(offset).is_none_or(|(_, bottom)| bottom >= screen.top())
    });
    let first_below = first_where(&within, |offset| {
        row_of(offset).is_some_and(|(top, _)| top > screen.bottom())
    });
    first_in_view..first_below.max(first_in_view)
}

/// The first offset in `within` where `holds` is true, or its end: `holds` must stay true once
/// it turns true.
fn first_where(within: &std::ops::Range<usize>, holds: impl Fn(usize) -> bool) -> usize {
    let (mut low, mut high) = (within.start, within.end);
    while low < high {
        let middle = low + (high - low) / 2;
        if holds(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

/// How far below the middle of a line the dots sit: under the letters, above the next line.
const SPELL_DOT_DROP: f32 = 10.;

/// The left and right edge of a mark on each row it covers, with that row's line.
fn mark_rows(
    state: &gpui_kit::base::input::EditorState,
    range: &std::ops::Range<usize>,
) -> Vec<(Pixels, Pixels, Bounds<Pixels>)> {
    let Some(text) = crate::spelling::text_in(state.text(), range) else {
        return Vec::new();
    };
    let mut rows: Vec<(Pixels, Pixels, Bounds<Pixels>)> = Vec::new();
    for word in word_spans(&text) {
        let word = range.start + word.start..range.start + word.end;
        let (Some(from), Some(to)) = (
            state.range_to_bounds(&(word.start..word.start)),
            state.range_to_bounds(&(word.end..word.end)),
        ) else {
            continue;
        };
        if (from.top() - to.top()).abs() >= px(1.) {
            continue;
        }
        match rows.last_mut() {
            Some(row) if (row.2.top() - from.top()).abs() < px(1.) => row.1 = to.left(),
            _ => rows.push((from.left(), to.left(), from)),
        }
    }
    rows
}

/// The byte ranges of the runs of text between spaces.
fn word_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut spans = Vec::new();
    let mut start = None;
    for (index, c) in text.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(from)) => {
                spans.push(from..index);
                start = None;
            }
            (false, None) => start = Some(index),
            _ => {}
        }
    }
    if let Some(from) = start {
        spans.push(from..text.len());
    }
    spans
}

pub const SPELL_MENU_WIDTH: Pixels = px(280.);

/// The menu of a spell checker mark, under the word.
pub fn spell_menu(id: impl Into<ElementId>, roles: Roles) -> Stateful<Div> {
    popover_frame(id, roles)
        .w(SPELL_MENU_WIDTH)
        .p(px(8.))
        .gap(px(4.))
}

/// What the spell checker found: the kind in its color, then the explanation.
/// The header of the spell checker's menu: the kind of mistake, then what is wrong.
pub fn spell_message(spelling: bool, message: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .px(px(8.))
        .pt(px(4.))
        .pb(px(4.))
        .type_small()
        .text_color(roles.muted)
        .child(
            div()
                .weight_strong()
                .text_color(if spelling { roles.danger } else { roles.accent })
                .child(if spelling { "Spelling" } else { "Grammar" }),
        )
        .child(message.into())
}

/// A choice in the spell checker's menu: a replacement, or a quieter action.
pub fn spell_row(
    id: impl Into<ElementId>,
    active: bool,
    quiet: bool,
    roles: Roles,
) -> Stateful<Div> {
    option_row(id, active, false, roles)
        .min_h(px(28.))
        .px(px(8.))
        .py(px(4.))
        .gap(px(8.))
        .justify_between()
        .when(quiet, |this| this.type_small().text_color(roles.muted))
}

pub fn spell_divider(roles: Roles) -> Div {
    div().h(px(1.)).w_full().bg(roles.fill)
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
    use super::{
        CaretInk, CaretSpot, SPELL_DOT, bytes_on_screen, caret_quads, dotted_line, word_spans,
    };
    use super::{
        Enter, cascade_delay, dot_levels, dot_opacity, enter_looks, enter_offset,
        enters_in_a_layer, grow_way, lift_at, panel_in_out, slide_at, takes_input,
    };
    use super::{breath_stops, dots_stops, gauge_stops};
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
    fn a_bar_waits_its_turn_then_grows_fast_and_lands() {
        use ginga_gpui::ginga::Motion;
        let ms = std::time::Duration::from_millis;
        let mut bar = Motion::at(0f32);
        bar.set_target(80., grow_way(2), ms(0));
        assert_eq!(bar.sample(ms(48)).value, 0.);
        assert!(bar.sample(ms(48 + 84)).value > 0.7 * 80.);
        assert!(bar.sample(ms(48 + 420)).settled);
        let mut last = Motion::at(0f32);
        last.set_target(80., grow_way(50), ms(0));
        assert_eq!(last.sample(ms(14 * 24)).value, 0.);
        assert!(last.sample(ms(14 * 24 + 10)).value > 0.);
    }

    #[test]
    fn a_panel_arrives_and_leaves_on_the_curves_it_always_had() {
        let ms = std::time::Duration::from_millis;
        let ways = panel_in_out();
        let mut panel = ginga_gpui::ginga::Presence::absent();
        panel.show((), ways.enter, ms(0));
        let arriving = panel.frame(ms(130)).map(|shown| shown.progress);
        let expo = ginga_gpui::Easing::OutExpo.at(0.5);
        assert!((arriving.unwrap() - expo).abs() < 1e-3);
        panel.hide(ways.exit, ms(1_000));
        let leaving = panel.frame(ms(1_090)).map(|shown| shown.progress);
        assert!((leaving.unwrap() - 0.5f32.powi(4)).abs() < 1e-3);
        assert!(panel.frame(ms(1_180)).is_none());
    }

    #[test]
    fn something_that_enters_starts_hidden_at_its_offset_and_rests_in_place() {
        use ginga_gpui::Look;
        assert_eq!(
            enter_looks(Enter::Slide(-8.)),
            (
                Look::new().offset(-8., 0.).opacity(0.),
                Look::new().offset(0., 0.).opacity(1.),
            )
        );
        assert_eq!(
            enter_looks(Enter::Rise(6.)).0,
            Look::new().offset(0., 6.).opacity(0.)
        );
        assert_eq!(
            enter_looks(Enter::Lift(-4.)).0,
            Look::new().offset(0., -4.).opacity(0.)
        );
    }

    #[test]
    fn only_an_absolutely_placed_element_enters_in_a_layer() {
        use gpui_kit::{Styled as _, div};
        assert!(enters_in_a_layer(div().absolute().left(px(40.)).style()));
        assert!(!enters_in_a_layer(div().relative().style()));
        assert!(!enters_in_a_layer(div().style()));
    }

    #[test]
    fn a_panel_slides_in_from_its_side_and_rests_in_place() {
        assert_eq!(slide_at(-16., 0.), (0., -16.));
        assert_eq!(slide_at(-16., 0.5), (0.5, -8.));
        assert_eq!(slide_at(16., 1.), (1., 0.));
        assert_eq!(slide_at(16., 1.5), (1., 0.));
    }

    #[test]
    fn a_breath_goes_down_to_045_and_back_in_1600_ms() {
        use ginga_gpui::ginga::Keyframes;
        let ms = std::time::Duration::from_millis;
        let breath = Keyframes::new(breath_stops()).unwrap().forever().mirrored();
        assert!((breath.sample(ms(0)).value - 1.).abs() < 1e-5);
        assert!((breath.sample(ms(800)).value - 0.45).abs() < 1e-5);
        assert!((breath.sample(ms(400)).value - 0.725).abs() < 1e-3);
        assert!((breath.sample(ms(1600)).value - 1.).abs() < 1e-5);
        let gauge = Keyframes::new(gauge_stops()).unwrap().forever().mirrored();
        assert!((gauge.sample(ms(1800)).value - 0.25).abs() < 1e-5);
        assert!((gauge.sample(ms(900)).value - 0.625).abs() < 1e-3);
        assert!((gauge.sample(ms(450)).value - (1. - 0.75 * 0.125)).abs() < 1e-3);
        let dots = Keyframes::new(dots_stops()).unwrap().forever();
        assert!((dots.sample(ms(300)).value - 0.25).abs() < 1e-5);
        assert!((dots.sample(ms(1500)).value - 0.25).abs() < 1e-5);
    }

    #[test]
    fn each_dot_rises_a_little_after_the_one_before() {
        let [first, second, third] = dot_levels(0.4, false);
        assert!((first - 1.).abs() < 1e-5);
        assert!(second < first && third < second);
        let later = dot_levels(0.4 + 160. / 1200., false);
        assert!((later[1] - 1.).abs() < 1e-5);
        assert_eq!(dot_levels(0.4, true), [1.; 3]);
    }

    #[test]
    fn a_dot_is_faint_at_the_ends_and_full_at_forty_percent() {
        assert!((dot_opacity(0.) - 0.35).abs() < 1e-5);
        assert!((dot_opacity(0.4) - 1.).abs() < 1e-5);
        assert!((dot_opacity(1.) - 0.35).abs() < 1e-5);
        assert!(dot_opacity(0.7) > 0.35 && dot_opacity(0.7) < 1.);
    }

    #[test]
    fn dots_fill_the_span_one_gap_apart() {
        let dots = dotted_line(px(10.), px(20.), px(5.));
        let lefts: Vec<f32> = dots.iter().map(|dot| f32::from(dot.left())).collect();
        assert_eq!(lefts, vec![10., 14., 18.]);
        assert!(
            dots.iter()
                .all(|dot| dot.top() == px(5.) && dot.size.width == SPELL_DOT)
        );
        assert!(dotted_line(px(10.), px(11.), px(0.)).is_empty());
    }

    #[test]
    fn word_spans_skip_the_spaces() {
        assert_eq!(word_spans("could  of"), vec![0..5, 7..9]);
        assert_eq!(word_spans(" não "), vec![1..5]);
        assert!(word_spans("   ").is_empty());
    }

    fn rows_of_20_bytes(
        line: f32,
    ) -> impl Fn(usize) -> Option<(gpui_kit::Pixels, gpui_kit::Pixels)> {
        move |offset| {
            let top = (offset / 20) as f32 * line;
            Some((px(top), px(top + line)))
        }
    }

    fn screen(top: f32, bottom: f32) -> gpui_kit::Bounds<gpui_kit::Pixels> {
        gpui_kit::Bounds::from_corners(point(px(0.), px(top)), point(px(100.), px(bottom)))
    }

    #[test]
    fn a_long_line_is_cut_to_the_rows_on_screen() {
        let bytes = bytes_on_screen(0..55_000, rows_of_20_bytes(20.), screen(2_000., 2_400.));
        assert_eq!(bytes, 1_980..2_420);
    }

    #[test]
    fn rows_not_laid_out_count_as_in_view() {
        let bytes = bytes_on_screen(0..100, |_| None, screen(0., 100.));
        assert_eq!(bytes, 0..100);
    }

    #[test]
    fn the_whole_range_stays_when_everything_is_on_screen() {
        let bytes = bytes_on_screen(0..100, rows_of_20_bytes(20.), screen(0., 500.));
        assert_eq!(bytes, 0..100);
    }

    #[test]
    fn an_empty_range_when_every_row_is_above_the_screen() {
        let bytes = bytes_on_screen(0..100, rows_of_20_bytes(20.), screen(1_000., 1_200.));
        assert_eq!(bytes, 100..100);
    }

    #[test]
    fn a_lift_fades_in_as_it_rises_into_place() {
        assert_eq!(lift_at(10., 0.), (0., 10.));
        assert_eq!(lift_at(10., 0.5), (0.5, 5.));
        assert_eq!(lift_at(10., 1.), (1., 0.));
    }

    #[test]
    fn the_toast_lifts_as_an_entering_lift_does() {
        for progress in [0., 0.5, 1.] {
            let entering = (progress, enter_offset(Enter::Lift(10.), 1. - progress).1);
            assert_eq!(lift_at(10., progress), entering);
        }
    }

    #[test]
    fn a_surface_takes_input_until_it_leaves() {
        use ginga_gpui::Phase;
        assert!(takes_input(Phase::Entering));
        assert!(takes_input(Phase::Present));
        assert!(!takes_input(Phase::Exiting));
    }
}
