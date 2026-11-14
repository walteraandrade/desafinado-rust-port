use std::{ops::Range, path::PathBuf, time::Instant};

use gpui_kit::{
    component::{
        ActiveTheme as _,
        button::{Button, ButtonVariants as _},
        input::{Editor, EditorState, RangeDecoration, RangeDecorationStyle, TextDecoration},
        h_flex, v_flex,
    },
    *,
};

actions!(spike, [AcceptSuggestion, RejectSuggestion, SaveDocument]);

struct Suggestion {
    range: Range<usize>,
    replacement: String,
}

struct Spike {
    path: PathBuf,
    editor: Entity<EditorState>,
    suggestion: Option<Suggestion>,
    strike: Option<gpui_kit::component::input::TextDecorationCollection>,
    frame: Option<gpui_kit::component::input::RangeDecorationCollection>,
    started: Instant,
    first_frame_logged: bool,
}

fn second_paragraph(text: &str) -> Option<Range<usize>> {
    let mut start = 0;
    let mut index = 0;
    for chunk in text.split("\n\n") {
        let trimmed = chunk.trim_end();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            index += 1;
            if index == 2 {
                return Some(start..start + trimmed.len());
            }
        }
        start += chunk.len() + 2;
    }
    None
}

impl Spike {
    fn new(path: PathBuf, started: Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let range = second_paragraph(&text);
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .soft_wrap(true);
            state.set_value(text.clone(), window, cx);
            state
        });
        let mut spike = Self {
            path,
            editor,
            suggestion: range.map(|range| Suggestion {
                range,
                replacement: "A shorter paragraph, proposed by the model.".into(),
            }),
            strike: None,
            frame: None,
            started,
            first_frame_logged: false,
        };
        spike.decorate(cx);
        spike
    }

    fn decorate(&mut self, cx: &mut Context<Self>) {
        let Some(suggestion) = &self.suggestion else {
            return;
        };
        let red = cx.theme().danger;
        let range = suggestion.range.clone();
        let (strike, frame) = self.editor.update(cx, |state, cx| {
            let strike = state.create_decorations_collection(
                vec![TextDecoration::new(
                    range.clone(),
                    HighlightStyle {
                        color: Some(red),
                        strikethrough: Some(StrikethroughStyle {
                            thickness: px(1.),
                            color: Some(red),
                        }),
                        ..Default::default()
                    },
                )],
                cx,
            );
            let frame = state.create_range_decorations_collection(
                vec![
                    RangeDecoration::new(range)
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(red.opacity(0.12)),
                ],
                cx,
            );
            (strike, frame)
        });
        self.strike = Some(strike);
        self.frame = Some(frame);
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        if let Some(strike) = self.strike.take() {
            strike.set(vec![], cx);
        }
        if let Some(frame) = self.frame.take() {
            frame.set(vec![], cx);
        }
        self.suggestion = None;
        cx.notify();
    }

    fn accept(&mut self, _: &AcceptSuggestion, window: &mut Window, cx: &mut Context<Self>) {
        let Some(suggestion) = self.suggestion.take() else {
            return;
        };
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(suggestion.range.clone(), cx);
            state.replace(suggestion.replacement.clone(), window, cx);
        });
        self.clear(cx);
    }

    fn reject(&mut self, _: &RejectSuggestion, _: &mut Window, cx: &mut Context<Self>) {
        self.clear(cx);
    }

    fn save(&mut self, _: &SaveDocument, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).value().to_string();
        let tmp = self.path.with_extension("md.tmp");
        if std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &self.path)).is_ok() {
            eprintln!("saved {}", self.path.display());
        }
    }

    fn current_range(&self, cx: &App) -> Option<Range<usize>> {
        let collection = self.frame.as_ref()?;
        collection.get_ranges(cx).into_iter().next()
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            let elapsed = self.started.elapsed();
            window.on_next_frame(move |_, _| {
                eprintln!("first frame after {:?}", elapsed);
            });
        }
        let card = self.suggestion.as_ref().and_then(|suggestion| {
            let range = self.current_range(cx).unwrap_or(suggestion.range.clone());
            let bounds = self.editor.read(cx).range_to_bounds(&range)?;
            Some(
                v_flex()
                    .absolute()
                    .left(bounds.left())
                    .top(bounds.bottom() + px(4.))
                    .w(px(420.))
                    .p_2()
                    .gap_2()
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().success)
                    .child(div().text_color(cx.theme().success).child(suggestion.replacement.clone()))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("accept")
                                    .primary()
                                    .label("Accept")
                                    .on_click(cx.listener(|this, _, window, cx| this.accept(&AcceptSuggestion, window, cx))),
                            )
                            .child(
                                Button::new("reject")
                                    .label("Reject")
                                    .on_click(cx.listener(|this, _, window, cx| this.reject(&RejectSuggestion, window, cx))),
                            ),
                    ),
            )
        });
        div()
            .id("spike")
            .key_context("Spike")
            .on_action(cx.listener(Self::accept))
            .on_action(cx.listener(Self::reject))
            .on_action(cx.listener(Self::save))
            .size_full()
            .bg(cx.theme().background)
            .child(Editor::new(&self.editor).h_full().bordered(false))
            .children(card)
    }
}

fn main() {
    let started = Instant::now();
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: gpui-spike <file.md>");
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-enter", AcceptSuggestion, Some("Spike")),
            KeyBinding::new("ctrl-backspace", RejectSuggestion, Some("Spike")),
            KeyBinding::new("ctrl-s", SaveDocument, Some("Spike")),
        ]);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1000.), px(700.)),
                cx,
            ))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Spike::new(path, started, window, cx))
        })
        .expect("failed to open window");
    });
}
