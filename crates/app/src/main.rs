mod document;
mod editor_pane;
mod frontmatter_view;
mod history_store;
mod history_view;
mod last_file;
mod library_panel;
mod new_skill_dialog;
mod single_instance;
mod theme;
mod ui;
mod workspace;

use std::{borrow::Cow, path::PathBuf};

use desafinado_platform::{launch::launch_file_of, user_data::user_data_dir};
use single_instance::{Claim, LaunchMessage};

use gpui_kit::*;

const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/LeagueSpartan.ttf"),
];

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let (sender, launches) = futures::channel::mpsc::unbounded::<LaunchMessage>();
    let message = LaunchMessage {
        argv: argv.clone(),
        cwd: cwd.clone(),
    };
    let instance = match single_instance::claim(&user_data_dir(), &message, move |message| {
        let _ = sender.unbounded_send(message);
    }) {
        Ok(Claim::HandedOver) => return,
        Ok(Claim::Primary(primary)) => Some(primary),
        Err(error) => {
            eprintln!("Could not claim the single instance: {error}");
            None
        }
    };
    let launch =
        launch_file_of(&argv, &cwd).map(|path| std::fs::canonicalize(&path).unwrap_or(path));
    let services = desafinado_runtime::services::Services::load();
    let _held = instance;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets::new(""))
        .run(move |cx| {
            gpui_kit::init(cx);
            let _ = cx
                .text_system()
                .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect());
            theme::apply(theme::mood_indigo(), cx);
            workspace::init(cx);
            editor_pane::init(cx);
            library_panel::init(cx);
            new_skill_dialog::init(cx);
            let mut options = component::TitleBar::window_options();
            options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.), px(860.)),
                cx,
            )));
            options.app_id = Some("desafinado".into());
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    workspace::Workspace::new(
                        workspace::Startup {
                            launch,
                            services,
                            launches,
                        },
                        window,
                        cx,
                    )
                })
            })
            .expect("failed to open the window");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
