//! Writes each panic to the crash log in the user data folder, keeping the default message
//! on stderr.

use std::{backtrace::Backtrace, path::Path};

use desafinado_platform::crash_log::{Crash, FILE_NAME, append, panic_message};

pub fn install(dir: &Path) {
    let path = dir.join(FILE_NAME);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info
            .location()
            .map_or_else(|| "an unknown place".to_string(), ToString::to_string);
        let entry = Crash {
            at: &chrono::Local::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION"),
            thread: thread.name().unwrap_or("unnamed"),
            location: &location,
            message: panic_message(info.payload()),
            backtrace: &Backtrace::force_capture().to_string(),
        }
        .entry();
        if let Err(error) = append(&path, &entry) {
            log::error!("Could not write {}: {error}", path.display());
        }
        previous(info);
    }));
}
