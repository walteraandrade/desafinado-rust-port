use desafinado_core::log_err::LogErr;
#[cfg(feature = "perf-probe")]
macro_rules! perf_span {
    ($name:expr) => {
        let _perf_span = crate::perf_probe::Span::new($name);
    };
}
#[cfg(not(feature = "perf-probe"))]
macro_rules! perf_span {
    ($name:expr) => {};
}
mod caret;
mod comment_review;
mod crash_log;
mod document;
mod editor_pane;
mod editor_place;
mod eval_run_dialog;
mod evals_view;
mod frontmatter_view;
mod history_store;
mod history_view;
mod jobs_tray;
mod key_dialog;
mod last_file;
mod library_panel;
mod logger;
mod menu_nav;
mod model_dialog;
mod new_skill_dialog;
#[cfg(feature = "perf-probe")]
mod perf_probe;
mod persist;
mod preferences_view;
mod reading_view;
mod rope_text;
mod single_instance;
mod spelling;
mod spend;
mod spend_view;
mod start_error;
mod text_map_view;
mod theme;
mod ui;
mod workspace;

use std::{borrow::Cow, path::PathBuf};

use desafinado_platform::{
    launch::launch_file_of,
    user_data::{ensure_user_data_dir, user_data_dir},
};
use single_instance::{Claim, LaunchMessage};

use gpui_kit::*;

const FONTS: [&[u8]; 7] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/LeagueSpartan.ttf"),
    include_bytes!("../assets/fonts/NotoSansMono-Regular.ttf"),
    include_bytes!("../assets/fonts/NotoSansMono-Bold.ttf"),
];

/// Every dialog, panel and popover fits inside this size.
const MIN_WINDOW: Size<Pixels> = size(px(720.), px(540.));

fn window_options(cx: &mut App) -> WindowOptions {
    let mut options = component::TitleBar::window_options();
    options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
        None,
        size(px(1280.), px(860.)),
        cx,
    )));
    options.window_min_size = Some(MIN_WINDOW);
    options.app_id = Some("desafinado-native".into());
    options
}

fn start_failure() -> Option<String> {
    let dir = user_data_dir();
    ensure_user_data_dir(&dir)
        .and_then(|()| {
            let probe = dir.join(".write-check");
            std::fs::write(&probe, b"")?;
            std::fs::remove_file(probe)
        })
        .err()
        .map(|error| {
            start_error::start_error_message(&format!(
                "Could not use the user data folder {}: {error}",
                dir.display()
            ))
        })
}

fn run_app<V: Render>(
    setup: impl FnOnce(&mut App) + 'static,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets::new(""))
        .run(move |cx| {
            {
                perf_span!("main.gpui_kit_init");
                gpui_kit::init(cx);
            }
            #[cfg(feature = "perf-probe")]
            perf_probe::start(cx);
            cx.text_system()
                .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
                .log_err();
            setup(cx);
            let options = window_options(cx);
            perf_span!("main.open_window");
            gpui_kit::open_window(options, cx, build).expect("failed to open the window");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
            spelling::preload(cx);
        });
}

fn show_start_error(message: String) {
    run_app(theme::apply_default, |_, cx| {
        cx.new(|_| start_error::StartError::new(message))
    });
}

fn main() {
    #[cfg(feature = "perf-probe")]
    perf_probe::mark_start();
    logger::init();
    if let Some(message) = start_failure() {
        log::error!("Failed to start: {message}");
        show_start_error(message);
        return;
    }
    crash_log::install(&user_data_dir());
    let argv: Vec<String> = std::env::args().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let (sender, launches) = futures::channel::mpsc::unbounded::<LaunchMessage>();
    let message = LaunchMessage {
        argv: argv.clone(),
        cwd: cwd.clone(),
    };
    let instance = match single_instance::claim(&user_data_dir(), &message, move |message| {
        sender.unbounded_send(message).log_warn();
    }) {
        Ok(Claim::HandedOver) => return,
        Ok(Claim::Primary(primary)) => Some(primary),
        Err(error) => {
            log::error!("Could not claim the single instance: {error}");
            None
        }
    };
    let launch =
        launch_file_of(&argv, &cwd).map(|path| std::fs::canonicalize(&path).unwrap_or(path));
    let services = {
        perf_span!("main.services_load");
        desafinado_runtime::services::Services::load()
    };
    let _held = instance;
    let user_data = services.user_data.clone();
    run_app(
        move |cx| {
            perf_span!("main.setup");
            caret::init(&user_data, cx);
            spelling::init(&user_data, cx);
            theme::init(&user_data, cx);
            workspace::init(cx);
            preferences_view::init(cx);
            key_dialog::init(cx);
            model_dialog::init(cx);
            editor_pane::init(cx);
            library_panel::init(cx);
            new_skill_dialog::init(cx);
            eval_run_dialog::init(cx);
        },
        move |window, cx| {
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
        },
    );
}
