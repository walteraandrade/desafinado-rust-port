//! Visible handling for failures nobody upstream can act on.
//!
//! `let _ = fallible();` hides a failure for good. These helpers keep going
//! the same way but leave a line in the log, at the caller's location.

use std::fmt::Display;
use std::panic::Location;

pub trait LogErr<T> {
    /// Logs the error at error level and returns the value, if any.
    fn log_err(self) -> Option<T>;
    /// Logs the error at warn level, for best-effort cleanup and for a peer that went away.
    fn log_warn(self) -> Option<T>;
}

impl<T, E: Display> LogErr<T> for Result<T, E> {
    #[track_caller]
    fn log_err(self) -> Option<T> {
        log_at(self, log::Level::Error, Location::caller())
    }

    #[track_caller]
    fn log_warn(self) -> Option<T> {
        log_at(self, log::Level::Warn, Location::caller())
    }
}

fn log_at<T, E: Display>(
    result: Result<T, E>,
    level: log::Level,
    caller: &Location<'_>,
) -> Option<T> {
    result
        .inspect_err(|error| {
            log::log!(level, "{error} ({}:{})", caller.file(), caller.line());
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_value_of_a_success() {
        let result: Result<u8, String> = Ok(7);
        assert_eq!(result.log_err(), Some(7));
    }

    #[test]
    fn turns_a_failure_into_none() {
        let result: Result<u8, String> = Err("disk full".into());
        assert_eq!(result.log_warn(), None);
    }
}
