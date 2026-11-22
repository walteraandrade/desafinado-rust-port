//! A stderr logger, so failures reach the journal when the app starts from its desktop entry.
//!
//! `DESAFINADO_LOG` sets the level for this app's crates (`error`, `warn`, `info`,
//! `debug`, `trace`; default `info`). Other crates log at `warn` and above.

use std::io::Write;

use log::{LevelFilter, Log, Metadata, Record};

const LEVEL_ENV: &str = "DESAFINADO_LOG";

struct StderrLogger {
    own: LevelFilter,
}

fn is_own(target: &str) -> bool {
    target.starts_with("desafinado")
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let limit = if is_own(metadata.target()) {
            self.own
        } else {
            LevelFilter::Warn
        };
        metadata.level() <= limit
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let mut stderr = std::io::stderr().lock();
            let line = format!(
                "[{} {}] {}\n",
                record.level(),
                record.target(),
                record.args()
            );
            // A logger has nowhere to report its own failure to write.
            stderr.write_all(line.as_bytes()).unwrap_or(());
        }
    }

    fn flush(&self) {}
}

fn level_from(value: Option<&str>) -> LevelFilter {
    value
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(LevelFilter::Info)
}

pub fn init() {
    let own = level_from(std::env::var(LEVEL_ENV).ok().as_deref());
    let logger = Box::new(StderrLogger { own });
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(own.max(LevelFilter::Warn));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_level_from_the_environment_value() {
        assert_eq!(level_from(Some("debug")), LevelFilter::Debug);
        assert_eq!(level_from(Some(" WARN ")), LevelFilter::Warn);
    }

    #[test]
    fn falls_back_to_info() {
        assert_eq!(level_from(None), LevelFilter::Info);
        assert_eq!(level_from(Some("loud")), LevelFilter::Info);
    }

    #[test]
    fn treats_only_this_app_as_its_own() {
        assert!(is_own("desafinado_platform::history::db"));
        assert!(!is_own("gpui::window"));
    }
}
