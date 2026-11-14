mod ai_backend;
mod editor_pane;
mod theme;
mod ui;
mod workspace;

use std::{borrow::Cow, path::PathBuf};

use gpui_kit::*;

const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/LeagueSpartan.ttf"),
];

fn main() {
    let launch: Option<PathBuf> = std::env::args().skip(1).find(|arg| !arg.starts_with('-')).map(PathBuf::from);
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let _ = cx
            .text_system()
            .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect());
        theme::apply(theme::mood_indigo(), cx);
        workspace::init(cx);
        editor_pane::init(cx);
        let mut options = component::TitleBar::window_options();
        options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1280.), px(860.)),
            cx,
        )));
        options.app_id = Some("desafinado".into());
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| workspace::Workspace::new(launch, window, cx))
        })
        .expect("failed to open the window");
        cx.activate(true);
    });
}
