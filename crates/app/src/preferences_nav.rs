//! Keyboard focus in Preferences: every control sits in a row, the arrow keys move between rows
//! and along a row, Tab walks every control, and Enter or Space presses the focused one.

use std::{cell::Cell, rc::Rc};

use desafinado_ai::quick_action_settings::{
    QuickActionDraft, is_user_quick_action, move_quick_action, remove_user_quick_action,
    toggle_quick_action,
};
use desafinado_features::caret::{CaretBlink, CaretSettings, CaretShape, MAX_WIDTH, MIN_WIDTH};
use desafinado_features::spelling::settings::{MenuKey, SpellSettings};
use desafinado_features::theme::{catalog::themes_of_mode, tokens::ThemeMode};
use gpui_kit::*;

use super::{Editing, PreferencesEvent, PreferencesPanel, draft_of};
use crate::{
    caret,
    menu_nav::{GridStep, grid_step, revealed_offset},
    spelling, theme,
};

actions!(
    preferences,
    [
        PrefUp,
        PrefDown,
        PrefLeft,
        PrefRight,
        PrefNext,
        PrefPrevious,
        PrefPress
    ]
);

const CONTEXT: &str = "Preferences && !Input";
const REVEAL_MARGIN: f32 = 8.;

pub(super) fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("up", PrefUp, Some(CONTEXT)),
        KeyBinding::new("down", PrefDown, Some(CONTEXT)),
        KeyBinding::new("left", PrefLeft, Some(CONTEXT)),
        KeyBinding::new("right", PrefRight, Some(CONTEXT)),
        KeyBinding::new("tab", PrefNext, Some(CONTEXT)),
        KeyBinding::new("shift-tab", PrefPrevious, Some(CONTEXT)),
        KeyBinding::new("enter", PrefPress, Some(CONTEXT)),
        KeyBinding::new("space", PrefPress, Some(CONTEXT)),
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Done,
    Theme(String),
    Choice(&'static str, SharedString),
    Setting(Setting),
    QuickToggle(String),
    QuickUp(String),
    QuickDown(String),
    QuickEdit(String),
    QuickDelete(String),
    QuickKeep(String),
    QuickConfirm(String),
    NewQuickAction,
}

impl Control {
    fn quick_action(&self) -> Option<&str> {
        match self {
            Self::QuickToggle(id)
            | Self::QuickUp(id)
            | Self::QuickDown(id)
            | Self::QuickEdit(id)
            | Self::QuickDelete(id)
            | Self::QuickKeep(id)
            | Self::QuickConfirm(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Setting {
    Model,
    Key,
    Spend,
}

impl Setting {
    pub(super) fn event(self) -> PreferencesEvent {
        match self {
            Self::Model => PreferencesEvent::OpenModel,
            Self::Key => PreferencesEvent::OpenKey,
            Self::Spend => PreferencesEvent::OpenSpend,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChoiceValue {
    Caret(CaretSettings),
    Spelling(SpellSettings),
}

impl ChoiceValue {
    pub(super) fn event(self) -> PreferencesEvent {
        match self {
            Self::Caret(settings) => PreferencesEvent::Caret(settings),
            Self::Spelling(settings) => PreferencesEvent::Spelling(settings),
        }
    }
}

pub(super) struct Choice {
    pub name: SharedString,
    pub value: ChoiceValue,
    pub pressed: bool,
    pub disabled: bool,
}

pub(super) struct ChoiceGroup {
    pub label: &'static str,
    pub choices: Vec<Choice>,
}

fn group<T: Copy + PartialEq>(
    label: &'static str,
    choices: Vec<(SharedString, T, bool)>,
    current: T,
    value: fn(T) -> ChoiceValue,
) -> ChoiceGroup {
    ChoiceGroup {
        label,
        choices: choices
            .into_iter()
            .map(|(name, choice, disabled)| Choice {
                name,
                value: value(choice),
                pressed: choice == current,
                disabled,
            })
            .collect(),
    }
}

pub(super) fn caret_groups(cx: &App) -> Vec<ChoiceGroup> {
    let current = caret::settings(cx);
    let shapes = CaretShape::ALL
        .iter()
        .map(|&shape| {
            (
                shape.label().into(),
                CaretSettings { shape, ..current },
                false,
            )
        })
        .collect();
    let widths = (MIN_WIDTH..=MAX_WIDTH)
        .map(|width| {
            (
                format!("{width} px").into(),
                current.with_width(width),
                !current.shape.has_width(),
            )
        })
        .collect();
    let blinks = CaretBlink::ALL
        .iter()
        .map(|&blink| {
            (
                blink.label().into(),
                CaretSettings { blink, ..current },
                false,
            )
        })
        .collect();
    vec![
        group("Shape", shapes, current, ChoiceValue::Caret),
        group("Width", widths, current, ChoiceValue::Caret),
        group("Blink", blinks, current, ChoiceValue::Caret),
    ]
}

pub(super) fn spelling_groups(cx: &App) -> Vec<ChoiceGroup> {
    let current = spelling::settings(cx);
    let switch = [("On", true), ("Off", false)]
        .into_iter()
        .map(|(name, enabled)| (name.into(), SpellSettings { enabled, ..current }, false))
        .collect();
    let keys = MenuKey::ALL
        .iter()
        .map(|&menu_key| {
            let settings = SpellSettings {
                menu_key,
                ..current
            };
            (menu_key.label().into(), settings, !current.enabled)
        })
        .collect();
    vec![
        group("Check spelling", switch, current, ChoiceValue::Spelling),
        group("Suggestions key", keys, current, ChoiceValue::Spelling),
    ]
}

pub(super) fn theme_ids(cx: &App) -> Vec<String> {
    let state = theme::state(cx);
    state
        .omarchy_option()
        .into_iter()
        .chain(themes_of_mode(ThemeMode::Dark).into_iter().cloned())
        .chain(themes_of_mode(ThemeMode::Light).into_iter().cloned())
        .map(|option| option.id.clone())
        .collect()
}

/// Where the keyboard is in Preferences, and whether the next frame should scroll it into view.
#[derive(Default)]
pub(super) struct KeyboardFocus {
    pub control: Option<Control>,
    at: Option<(usize, usize)>,
    pub reveal: Rc<Cell<bool>>,
    pub form_reveal: Rc<Cell<bool>>,
}

impl PreferencesPanel {
    pub(super) fn keyboard_on(&self, control: &Control) -> bool {
        self.keyboard.control.as_ref() == Some(control)
    }

    fn control_rows(&self, cx: &App) -> Vec<Vec<Control>> {
        let mut rows = vec![vec![Control::Done]];
        rows.extend(theme_ids(cx).into_iter().map(|id| vec![Control::Theme(id)]));
        for group in caret_groups(cx).into_iter().chain(spelling_groups(cx)) {
            let label = group.label;
            rows.push(
                group
                    .choices
                    .into_iter()
                    .filter(|choice| !choice.disabled)
                    .map(|choice| Control::Choice(label, choice.name))
                    .collect(),
            );
        }
        rows.extend(
            [Setting::Model, Setting::Key, Setting::Spend]
                .map(|setting| vec![Control::Setting(setting)]),
        );
        rows.extend(self.quick_action_rows());
        if matches!(self.editing, Editing::None | Editing::Delete(_)) {
            rows.push(vec![Control::NewQuickAction]);
        }
        rows
    }

    fn quick_action_rows(&self) -> Vec<Vec<Control>> {
        let last = self.settings.len().saturating_sub(1);
        self.settings
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let id = slot.action.id.clone();
                match &self.editing {
                    Editing::Edit(editing) if *editing == id => return None,
                    Editing::Delete(deleting) if *deleting == id => {
                        return Some(vec![
                            Control::QuickKeep(id.clone()),
                            Control::QuickConfirm(id),
                        ]);
                    }
                    _ => {}
                }
                let mine = is_user_quick_action(&slot.action);
                let busy = matches!(self.editing, Editing::New | Editing::Edit(_));
                let row = [
                    Some(Control::QuickToggle(id.clone())),
                    (index > 0).then(|| Control::QuickUp(id.clone())),
                    (index < last).then(|| Control::QuickDown(id.clone())),
                    (mine && !busy).then(|| Control::QuickEdit(id.clone())),
                    mine.then_some(Control::QuickDelete(id)),
                ];
                Some(row.into_iter().flatten().collect())
            })
            .collect()
    }

    fn step(&mut self, step: GridStep, cx: &mut Context<Self>) {
        let rows = self.control_rows(cx);
        let found = self.keyboard.control.as_ref().and_then(|control| {
            rows.iter().enumerate().find_map(|(row, controls)| {
                controls
                    .iter()
                    .position(|c| c == control)
                    .map(|column| (row, column))
            })
        });
        let at = match (found, &self.keyboard.control) {
            (Some(at), _) => Some(at),
            (None, Some(_)) => self.keyboard.at.map(|(row, column)| {
                let row = row.min(rows.len().saturating_sub(1));
                (row, column.min(rows[row].len().saturating_sub(1)))
            }),
            (None, None) => {
                let chosen = Control::Theme(theme::state(cx).selected());
                self.focus_control(&rows, &chosen, cx);
                return;
            }
        };
        let lengths: Vec<usize> = rows.iter().map(Vec::len).collect();
        if let Some((row, column)) = grid_step(&lengths, at, step) {
            let control = rows[row][column].clone();
            self.focus_control(&rows, &control, cx);
        }
    }

    fn focus_control(&mut self, rows: &[Vec<Control>], control: &Control, cx: &mut Context<Self>) {
        self.keyboard.at = rows.iter().enumerate().find_map(|(row, controls)| {
            controls
                .iter()
                .position(|c| c == control)
                .map(|column| (row, column))
        });
        self.keyboard.control = Some(control.clone());
        self.keyboard.reveal.set(true);
        cx.notify();
    }

    pub(super) fn settle_after_keep(&mut self) {
        self.editing = Editing::None;
        self.keyboard.reveal.set(self.keyboard.control.is_some());
    }

    pub(super) fn forget_keyboard(&mut self, cx: &mut Context<Self>) {
        if self.keyboard.control.take().is_some() {
            cx.notify();
        }
    }

    fn press(&mut self, _: &PrefPress, window: &mut Window, cx: &mut Context<Self>) {
        let Some(control) = self.keyboard.control.clone() else {
            return;
        };
        match control {
            Control::Done => cx.emit(PreferencesEvent::Close),
            Control::Theme(id) => theme::choose(&id, cx),
            Control::Choice(label, name) => {
                let value = caret_groups(cx)
                    .into_iter()
                    .chain(spelling_groups(cx))
                    .filter(|group| group.label == label)
                    .flat_map(|group| group.choices)
                    .find(|choice| choice.name == name && !choice.disabled)
                    .map(|choice| choice.value);
                if let Some(value) = value {
                    cx.emit(value.event());
                }
            }
            Control::Setting(setting) => cx.emit(setting.event()),
            Control::NewQuickAction => self.start_draft(
                Editing::New,
                QuickActionDraft {
                    label: String::new(),
                    instruction: String::new(),
                    skill_only: false,
                },
                window,
                cx,
            ),
            quick => self.press_quick_action(quick, window, cx),
        }
        self.settle_keyboard(cx);
        self.keyboard.reveal.set(true);
        cx.notify();
    }

    /// Moves the keyboard to the control now at its place when a press removed the focused one.
    fn settle_keyboard(&mut self, cx: &App) {
        if matches!(self.editing, Editing::New | Editing::Edit(_)) {
            return;
        }
        let rows = self.control_rows(cx);
        let Some(control) = &self.keyboard.control else {
            return;
        };
        if rows.iter().flatten().any(|c| c == control) {
            return;
        }
        let Some((row, column)) = self.keyboard.at else {
            return;
        };
        let same_quick_action = control.quick_action().and_then(|id| {
            rows.iter()
                .position(|controls| controls.iter().any(|c| c.quick_action() == Some(id)))
        });
        let row = same_quick_action.unwrap_or(row.min(rows.len().saturating_sub(1)));
        if let Some(controls) = rows.get(row)
            && let Some(settled) = controls.get(column.min(controls.len().saturating_sub(1)))
        {
            self.keyboard.control = Some(settled.clone());
            self.keyboard.reveal.set(true);
        }
    }

    pub(super) fn press_quick_action(
        &mut self,
        control: Control,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match control {
            Control::QuickToggle(id) => {
                let on = self
                    .settings
                    .iter()
                    .any(|slot| slot.action.id == id && slot.on);
                self.change(|s| toggle_quick_action(s, &id, !on), cx);
            }
            Control::QuickUp(id) => self.change(|s| move_quick_action(s, &id, -1), cx),
            Control::QuickDown(id) => self.change(|s| move_quick_action(s, &id, 1), cx),
            Control::QuickEdit(id) => {
                if let Some(slot) = self.settings.iter().find(|slot| slot.action.id == id) {
                    let draft = draft_of(slot);
                    self.start_draft(Editing::Edit(id), draft, window, cx);
                }
            }
            Control::QuickDelete(id) => {
                self.editing = Editing::Delete(id.clone());
                self.draft = None;
                if self.keyboard.control.is_some() {
                    self.keyboard.control = Some(Control::QuickKeep(id));
                }
            }
            Control::QuickKeep(id) => {
                self.editing = Editing::None;
                if self.keyboard.control.is_some() {
                    self.keyboard.control = Some(Control::QuickDelete(id));
                }
            }
            Control::QuickConfirm(id) => {
                self.editing = Editing::None;
                self.change(|s| remove_user_quick_action(s, &id), cx);
            }
            _ => {}
        }
    }

    pub(super) fn on_keyboard_actions(
        &self,
        element: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        element
            .on_action(cx.listener(|this, _: &PrefUp, _, cx| this.step(GridStep::Up, cx)))
            .on_action(cx.listener(|this, _: &PrefDown, _, cx| this.step(GridStep::Down, cx)))
            .on_action(cx.listener(|this, _: &PrefLeft, _, cx| this.step(GridStep::Left, cx)))
            .on_action(cx.listener(|this, _: &PrefRight, _, cx| this.step(GridStep::Right, cx)))
            .on_action(cx.listener(|this, _: &PrefNext, _, cx| this.step(GridStep::Next, cx)))
            .on_action(cx.listener(|this, _: &PrefPrevious, _, cx| {
                this.step(GridStep::Previous, cx);
            }))
            .on_action(cx.listener(Self::press))
    }

    /// An invisible layer over the focused control that scrolls it into view once after a key moves it.
    pub(super) fn reveal_marker(&self) -> impl IntoElement + use<> {
        self.marker_for(Rc::clone(&self.keyboard.reveal))
    }

    /// Scrolls a Quick action form into view once, when it opens.
    pub(super) fn form_reveal_marker(&self) -> impl IntoElement + use<> {
        self.marker_for(Rc::clone(&self.keyboard.form_reveal))
    }

    fn marker_for(&self, pending: Rc<Cell<bool>>) -> impl IntoElement + use<> {
        let scroll = self.scroll.clone();
        canvas(
            move |bounds, window, _| {
                if !pending.replace(false) {
                    return;
                }
                let view = scroll.bounds();
                let offset = scroll.offset();
                if let Some(y) = revealed_offset(
                    f32::from(offset.y),
                    (
                        f32::from(view.top()) + REVEAL_MARGIN,
                        f32::from(view.bottom()) - REVEAL_MARGIN,
                    ),
                    (f32::from(bounds.top()), f32::from(bounds.bottom())),
                    f32::from(scroll.max_offset().y),
                ) {
                    scroll.set_offset(point(offset.x, px(y)));
                    window.refresh();
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_full()
    }
}
