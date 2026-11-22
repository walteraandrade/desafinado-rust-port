//! The writer's caret choice: loaded at start, applied to the editor's own
//! caret, and saved when Preferences change it.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use desafinado_features::caret::{
    CARET_FILE_NAME, CaretSettings, CaretShape, caret_shown, until_next_phase,
};
use desafinado_platform::files::write_atomic_unconditionally;
use gpui_kit::{
    App, Context, Entity, Global, Task,
    base::input::{
        CursorBlink, EditorState, RangeDecoration, RangeDecorationCollection, RangeDecorationStyle,
        TextDecoration, TextDecorationCollection,
    },
    component::Theme,
};

use crate::{editor_pane::EditorPane, theme, ui};

#[derive(Clone, Debug)]
pub struct CaretChoice {
    pub settings: CaretSettings,
    pub file: PathBuf,
}

impl Global for CaretChoice {}

pub fn init(user_data: &Path, cx: &mut App) {
    let file = user_data.join(CARET_FILE_NAME);
    let settings = match std::fs::read_to_string(&file) {
        Ok(text) => CaretSettings::parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => CaretSettings::default(),
        Err(error) => {
            log::warn!("The caret settings could not be read: {error}");
            CaretSettings::default()
        }
    };
    cx.set_global(CaretChoice { settings, file });
}

pub fn settings(cx: &App) -> CaretSettings {
    cx.try_global::<CaretChoice>()
        .map(|choice| choice.settings)
        .unwrap_or_default()
}

/// The blink of every other text field follows the writer's choice too.
pub fn apply_to_theme(theme: &mut Theme, settings: CaretSettings) {
    theme.cursor_blink = settings
        .blink
        .phase()
        .map_or_else(CursorBlink::steady, CursorBlink::blinking);
}

/// Stores the choice and applies it; returns the write to run off the UI thread.
pub fn choose(
    settings: CaretSettings,
    cx: &mut App,
) -> impl FnOnce() -> std::io::Result<()> + Send + 'static {
    let file = cx.global::<CaretChoice>().file.clone();
    cx.set_global(CaretChoice {
        settings,
        file: file.clone(),
    });
    Theme::update(cx, |theme| apply_to_theme(theme, settings));
    cx.refresh_windows();
    let text = settings.to_json();
    move || write_atomic_unconditionally(&file, &text)
}

/// The writer's caret in the document: its blink and, for the block shape,
/// the decorations that paint the character under it.
pub struct CaretMarks {
    fill: RangeDecorationCollection,
    ink: TextDecorationCollection,
    since: Instant,
    shown: bool,
}

impl CaretMarks {
    pub fn new(editor: &Entity<EditorState>, cx: &mut App) -> Self {
        let (fill, ink) = editor.update(cx, |state, cx| {
            (
                state.create_range_decorations_collection(vec![], cx),
                state.create_decorations_collection(vec![], cx),
            )
        });
        Self {
            fill,
            ink,
            since: Instant::now(),
            shown: true,
        }
    }

    pub fn shown(&self) -> bool {
        self.shown
    }

    /// The writer typed or moved: show the caret and start its blink over.
    pub fn restart(&mut self, editor: &Entity<EditorState>, cx: &mut App) {
        self.since = Instant::now();
        self.shown = true;
        self.refresh(editor, cx);
    }

    /// Shows or hides the caret for this moment; returns whether that changed
    /// and how long until it next might.
    pub fn tick(&mut self, editor: &Entity<EditorState>, cx: &mut App) -> (bool, Duration) {
        let blink = settings(cx).blink;
        let since = self.since.elapsed();
        let shown = caret_shown(blink, since);
        let changed = shown != self.shown;
        if changed {
            self.shown = shown;
            self.refresh(editor, cx);
        }
        let wait = until_next_phase(blink, since).unwrap_or(IDLE_CHECK) + PHASE_MARGIN;
        (changed, wait)
    }

    pub fn refresh(&self, editor: &Entity<EditorState>, cx: &mut App) {
        let state = editor.read(cx);
        let block = settings(cx).shape == CaretShape::Block
            && self.shown
            && state.selected_range().is_empty();
        let next = block
            .then(|| ui::next_character(state.text(), state.cursor()))
            .flatten();
        let (color, style) = ui::caret_block_ink(theme::roles(cx));
        let fill = next
            .clone()
            .map(|range| {
                RangeDecoration::new(range)
                    .with_style(RangeDecorationStyle::Fill)
                    .with_color(color)
            })
            .into_iter()
            .collect();
        let ink = next
            .map(|range| TextDecoration::new(range, style))
            .into_iter()
            .collect();
        self.fill.set(fill, cx);
        self.ink.set(ink, cx);
    }
}

const IDLE_CHECK: Duration = Duration::from_millis(500);
const PHASE_MARGIN: Duration = Duration::from_millis(4);

/// Wakes at each blink phase of the document's caret and redraws it.
pub fn start_blink(cx: &mut Context<EditorPane>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            let Ok(wait) = this.update(cx, |pane, cx| {
                let editor = pane.editor.clone();
                let (changed, wait) = pane.caret.tick(&editor, cx);
                if changed {
                    cx.notify();
                }
                wait
            }) else {
                break;
            };
            cx.background_executor().timer(wait).await;
        }
    })
}
